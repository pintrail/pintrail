import {
  BadRequestException,
  Injectable,
  NotFoundException,
  OnModuleDestroy,
} from '@nestjs/common';
import { InjectRepository } from '@nestjs/typeorm';
import { mkdir, rm, writeFile } from 'fs/promises';
import { extname, join } from 'path';
import { Queue } from 'bullmq';
import { In, Repository } from 'typeorm';
import { ArtifactImageEntity } from './artifact-image.entity';
import { ArtifactEntity } from './artifact.entity';
import { Artifact, ArtifactDetail, ArtifactImage, GeoCoordinates } from './artifact.types';
import { CreateArtifactDto } from './dto/create-artifact.dto';
import { UpdateArtifactDto } from './dto/update-artifact.dto';

@Injectable()
export class ArtifactsService implements OnModuleDestroy {
  private readonly queueName = process.env.IMAGE_QUEUE_NAME ?? 'artifact-image-processing';
  private readonly imageRoot =
    process.env.IMAGE_STORAGE_ROOT ?? join(process.cwd(), 'data', 'images');
  private readonly originalsDir = join(this.imageRoot, 'originals');
  private readonly processedDir = join(this.imageRoot, 'processed');
  private readonly imageQueue = new Queue(this.queueName, {
    connection: {
      host: process.env.REDIS_HOST ?? 'localhost',
      port: Number(process.env.REDIS_PORT ?? 6379),
    },
  });

  constructor(
    @InjectRepository(ArtifactEntity)
    private readonly artifactsRepository: Repository<ArtifactEntity>,
    @InjectRepository(ArtifactImageEntity)
    private readonly artifactImagesRepository: Repository<ArtifactImageEntity>,
  ) {}

  async onModuleDestroy() {
    await this.imageQueue.close();
  }

  async create(dto: CreateArtifactDto): Promise<Artifact> {
    const parentId = dto.parentId?.trim() || null;
    const parentArtifact = parentId ? await this.requireArtifactEntity(parentId) : null;

    const artifact = this.artifactsRepository.create({
      name: '',
      desc: '',
      lat: parentArtifact?.lat ?? null,
      lng: parentArtifact?.lng ?? null,
      parentId,
    });

    const savedArtifact = await this.artifactsRepository.save(artifact);
    return this.toArtifact(savedArtifact);
  }

  async findAll(): Promise<Artifact[]> {
    const artifacts = await this.artifactsRepository.find({
      order: { createdAt: 'ASC' },
    });

    return artifacts.map(artifact => this.toArtifact(artifact));
  }

  async findOne(id: string): Promise<ArtifactDetail> {
    const artifact = await this.requireArtifactEntity(id);
    const [children, images] = await Promise.all([
      this.artifactsRepository.find({
        where: { parentId: id },
        order: { createdAt: 'ASC' },
      }),
      this.findImageEntities(id),
    ]);

    return {
      ...this.toArtifact(artifact),
      children: children.map(child => this.toArtifact(child)),
      images: images.map(image => this.toArtifactImage(image)),
    };
  }

  async findImages(id: string): Promise<{ images: ArtifactImage[] }> {
    await this.requireArtifactEntity(id);
    const images = await this.findImageEntities(id);
    return { images: images.map(image => this.toArtifactImage(image)) };
  }

  async addImages(
    artifactId: string,
    files: Express.Multer.File[],
  ): Promise<{ images: ArtifactImage[] }> {
    await this.requireArtifactEntity(artifactId);

    if (!files.length) {
      throw new BadRequestException('At least one image file is required.');
    }

    await this.ensureStorageDirectories();

    const images = await Promise.all(
      files.map(async file => {
        if (!file.mimetype.startsWith('image/')) {
          throw new BadRequestException(`"${file.originalname}" is not an image.`);
        }

        const image = this.artifactImagesRepository.create({
          artifactId,
          originalFilename: file.originalname,
          originalMimeType: file.mimetype,
          originalStoragePath: '',
          status: 'queued',
        });
        const savedImage = await this.artifactImagesRepository.save(image);
        const extension = this.normalizeExtension(file.originalname);
        const originalRelativePath = join('originals', `${savedImage.id}${extension}`);
        const originalAbsolutePath = join(this.imageRoot, originalRelativePath);

        await writeFile(originalAbsolutePath, file.buffer);

        savedImage.originalStoragePath = originalRelativePath;
        const updatedImage = await this.artifactImagesRepository.save(savedImage);

        await this.imageQueue.add(
          'normalize-image',
          {
            imageId: updatedImage.id,
            originalPath: originalRelativePath,
          },
          {
            attempts: 3,
            backoff: {
              type: 'exponential',
              delay: 1000,
            },
          },
        );

        return this.toArtifactImage(updatedImage);
      }),
    );

    return { images };
  }

  async update(id: string, dto: UpdateArtifactDto): Promise<ArtifactDetail> {
    const artifact = await this.requireArtifactEntity(id);

    if (dto.parentId !== undefined) {
      const nextParentId = dto.parentId?.trim() || null;

      if (nextParentId === id) {
        throw new BadRequestException('An artifact cannot be its own parent.');
      }

      if (nextParentId) {
        await this.requireArtifactEntity(nextParentId);
      }

      if (nextParentId && (await this.isDescendant(nextParentId, id))) {
        throw new BadRequestException(
          'An artifact cannot be moved underneath one of its descendants.',
        );
      }

      artifact.parentId = nextParentId;
    }

    if (dto.name !== undefined) {
      artifact.name = dto.name;
    }

    if (dto.desc !== undefined) {
      artifact.desc = dto.desc;
    }

    if (dto.clearLocation) {
      artifact.lat = null;
      artifact.lng = null;
    } else if (dto.lat !== undefined || dto.lng !== undefined) {
      const coordinates = this.normalizeCoordinates(dto.lat, dto.lng);
      artifact.lat = coordinates?.lat ?? null;
      artifact.lng = coordinates?.lng ?? null;
    }

    await this.artifactsRepository.save(artifact);

    return this.findOne(id);
  }

  async removeImage(
    artifactId: string,
    imageId: string,
  ): Promise<{ deletedImageId: string }> {
    await this.requireArtifactEntity(artifactId);

    const image = await this.artifactImagesRepository.findOneBy({
      id: imageId,
      artifactId,
    });

    if (!image) {
      throw new NotFoundException(`Artifact image ${imageId} was not found.`);
    }

    await this.artifactImagesRepository.delete(imageId);

    const storagePaths = [image.originalStoragePath, image.processedFilename].filter(
      (storagePath): storagePath is string => Boolean(storagePath),
    );
    await Promise.all(
      storagePaths.map(storagePath => rm(join(this.imageRoot, storagePath), { force: true })),
    );

    return { deletedImageId: imageId };
  }

  async remove(id: string): Promise<{ deletedIds: string[] }> {
    await this.requireArtifactEntity(id);

    const artifacts = await this.artifactsRepository.find({
      select: {
        id: true,
        parentId: true,
      },
    });
    const deletedIds: string[] = [];
    this.collectDescendantsForDeletion(
      id,
      new Map(artifacts.map(artifact => [artifact.id, artifact.parentId])),
      deletedIds,
    );

    const images = await this.artifactImagesRepository.find({
      where: { artifactId: In(deletedIds) },
    });

    await this.artifactsRepository.delete(id);
    await Promise.all(
      images.flatMap(image => {
        const paths = [image.originalStoragePath, image.processedFilename].filter(
          (storagePath): storagePath is string => Boolean(storagePath),
        );
        return paths.map(storagePath =>
          rm(join(this.imageRoot, storagePath), { force: true }),
        );
      }),
    );

    return { deletedIds };
  }

  private async requireArtifactEntity(id: string): Promise<ArtifactEntity> {
    const artifact = await this.artifactsRepository.findOneBy({ id });
    if (!artifact) {
      throw new NotFoundException(`Artifact ${id} was not found.`);
    }

    return artifact;
  }

  private async isDescendant(
    candidateId: string,
    ancestorId: string,
  ): Promise<boolean> {
    const artifacts = await this.artifactsRepository.find({
      select: {
        id: true,
        parentId: true,
      },
    });
    const parentById = new Map(artifacts.map(artifact => [artifact.id, artifact.parentId]));
    let currentId: string | null | undefined = candidateId;

    while (currentId) {
      if (currentId === ancestorId) {
        return true;
      }

      currentId = parentById.get(currentId);
    }

    return false;
  }

  private collectDescendantsForDeletion(
    id: string,
    parentById: Map<string, string | null>,
    deletedIds: string[],
  ) {
    const childIds = [...parentById.entries()]
      .filter(([, parentId]) => parentId === id)
      .map(([artifactId]) => artifactId);

    for (const childId of childIds) {
      this.collectDescendantsForDeletion(childId, parentById, deletedIds);
    }

    deletedIds.push(id);
  }

  private normalizeCoordinates(
    lat: number | undefined,
    lng: number | undefined,
  ): GeoCoordinates | null {
    if (lat === undefined && lng === undefined) {
      return null;
    }

    if (lat === undefined || lng === undefined) {
      throw new BadRequestException(
        'Latitude and longitude must be provided together.',
      );
    }

    return { lat, lng };
  }

  private toArtifact(entity: ArtifactEntity): Artifact {
    return {
      id: entity.id,
      name: entity.name,
      desc: entity.desc,
      loc:
        entity.lat === null || entity.lng === null
          ? null
          : { lat: entity.lat, lng: entity.lng },
      parentId: entity.parentId,
      createdAt: entity.createdAt.toISOString(),
      updatedAt: entity.updatedAt.toISOString(),
    };
  }

  private toArtifactImage(entity: ArtifactImageEntity): ArtifactImage {
    return {
      id: entity.id,
      artifactId: entity.artifactId,
      originalFilename: entity.originalFilename,
      status: entity.status,
      url: entity.processedFilename ? this.toMediaUrl(entity.processedFilename) : null,
      width: entity.width,
      height: entity.height,
      errorMessage: entity.errorMessage,
      createdAt: entity.createdAt.toISOString(),
      updatedAt: entity.updatedAt.toISOString(),
    };
  }

  private findImageEntities(artifactId: string): Promise<ArtifactImageEntity[]> {
    return this.artifactImagesRepository.find({
      where: { artifactId },
      order: { createdAt: 'ASC' },
    });
  }

  private async ensureStorageDirectories() {
    await Promise.all([
      mkdir(this.originalsDir, { recursive: true }),
      mkdir(this.processedDir, { recursive: true }),
    ]);
  }

  private normalizeExtension(filename: string): string {
    const extension = extname(filename).toLowerCase();
    return extension || '.bin';
  }

  private toMediaUrl(storagePath: string): string {
    return `/media/${storagePath.split('\\').join('/')}`;
  }
}
