import { Inject, Injectable, NotFoundException } from '@nestjs/common';
import {
  ArtifactAssetEntity,
  ArtifactEntity,
  ArtifactKind,
} from './entities/artifact.entity';
import {
  CreateArtifactAssetDto,
  CreateArtifactDto,
} from './dto/create-artifact.dto';
import { UpdateArtifactDto } from './dto/update-artifact.dto';
import { ARTIFACT_REPOSITORY } from './repositories/artifact.repository';
import type { ArtifactRepository } from './repositories/artifact.repository';

@Injectable()
export class ArtifactsService {
  constructor(
    @Inject(ARTIFACT_REPOSITORY)
    private readonly artifactRepository: ArtifactRepository,
  ) {}

  async create(createArtifactDto: CreateArtifactDto): Promise<ArtifactEntity> {
    const artifact = new ArtifactEntity();

    artifact.kind = createArtifactDto.kind ?? ArtifactKind.ITEM;
    artifact.name = createArtifactDto.name;
    artifact.description = createArtifactDto.description ?? null;
    artifact.tags = createArtifactDto.tags ?? [];
    artifact.location = { ...(createArtifactDto.location ?? {}) };
    artifact.parentArtifactId = createArtifactDto.parentArtifactId ?? null;
    artifact.assets = (createArtifactDto.assets ?? []).map((assetDto) =>
      this.toAssetEntity(assetDto),
    );
    artifact.isActive = createArtifactDto.isActive ?? true;
    artifact.metadata = createArtifactDto.metadata ?? null;
    artifact.children = [];

    return this.artifactRepository.save(artifact);
  }

  async findAll(): Promise<ArtifactEntity[]> {
    return this.artifactRepository.findAll();
  }

  async findOne(id: string): Promise<ArtifactEntity> {
    const artifact = await this.artifactRepository.findById(id);
    if (!artifact) {
      throw new NotFoundException(`Artifact ${id} not found`);
    }
    return artifact;
  }

  async update(
    id: string,
    updateArtifactDto: UpdateArtifactDto,
  ): Promise<ArtifactEntity> {
    const existing = await this.findOne(id);

    const updated: ArtifactEntity = {
      ...existing,
      kind: updateArtifactDto.kind ?? existing.kind,
      name: updateArtifactDto.name ?? existing.name,
      description:
        updateArtifactDto.description === undefined
          ? existing.description
          : updateArtifactDto.description,
      tags: updateArtifactDto.tags ?? existing.tags,
      location:
        updateArtifactDto.location === undefined
          ? existing.location
          : { ...updateArtifactDto.location },
      parentArtifactId:
        updateArtifactDto.parentArtifactId === undefined
          ? existing.parentArtifactId
          : updateArtifactDto.parentArtifactId,
      assets:
        updateArtifactDto.assets === undefined
          ? existing.assets
          : updateArtifactDto.assets.map((assetDto) =>
              this.toAssetEntity(assetDto),
            ),
      isActive: updateArtifactDto.isActive ?? existing.isActive,
      metadata:
        updateArtifactDto.metadata === undefined
          ? existing.metadata
          : updateArtifactDto.metadata,
    };

    return this.artifactRepository.save(updated);
  }

  async remove(id: string): Promise<void> {
    const deleted = await this.artifactRepository.deleteById(id);
    if (!deleted) {
      throw new NotFoundException(`Artifact ${id} not found`);
    }
  }

  private toAssetEntity(assetDto: CreateArtifactAssetDto): ArtifactAssetEntity {
    const asset = new ArtifactAssetEntity();
    asset.type = assetDto.type;
    asset.title = assetDto.title ?? null;
    asset.description = assetDto.description ?? null;
    asset.sortOrder = assetDto.sortOrder ?? 0;
    asset.textContent = assetDto.textContent ?? null;
    asset.textFormat = assetDto.textFormat ?? null;
    asset.url = assetDto.url ?? null;
    asset.mimeType = assetDto.mimeType ?? null;
    asset.durationSeconds = assetDto.durationSeconds ?? null;
    asset.width = assetDto.width ?? null;
    asset.height = assetDto.height ?? null;
    asset.sizeBytes = assetDto.sizeBytes ?? null;
    asset.linkedArtifactId = assetDto.linkedArtifactId ?? null;
    asset.relationship = assetDto.relationship ?? null;
    asset.metadata = assetDto.metadata ?? null;
    return asset;
  }
}
