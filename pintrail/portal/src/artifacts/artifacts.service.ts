import {
  BadRequestException,
  Injectable,
  NotFoundException,
} from '@nestjs/common';
import { InjectRepository } from '@nestjs/typeorm';
import { Repository } from 'typeorm';
import { ArtifactEntity } from './artifact.entity';
import { Artifact, ArtifactDetail, GeoCoordinates } from './artifact.types';
import { CreateArtifactDto } from './dto/create-artifact.dto';
import { UpdateArtifactDto } from './dto/update-artifact.dto';

@Injectable()
export class ArtifactsService {
  constructor(
    @InjectRepository(ArtifactEntity)
    private readonly artifactsRepository: Repository<ArtifactEntity>,
  ) {}

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
    const children = await this.artifactsRepository.find({
      where: { parentId: id },
      order: { createdAt: 'ASC' },
    });

    return {
      ...this.toArtifact(artifact),
      children: children.map(child => this.toArtifact(child)),
    };
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

    await this.artifactsRepository.delete(id);

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
}
