import { Injectable } from '@nestjs/common';
import { randomUUID } from 'node:crypto';
import {
  ArtifactAssetEntity,
  ArtifactEntity,
} from '../entities/artifact.entity';
import { ArtifactRepository } from './artifact.repository';

@Injectable()
export class InMemoryArtifactRepository implements ArtifactRepository {
  private readonly store = new Map<string, ArtifactEntity>();

  save(artifact: ArtifactEntity): Promise<ArtifactEntity> {
    const now = new Date();
    const existing = artifact.id ? this.store.get(artifact.id) : undefined;
    const id = artifact.id || randomUUID();

    const nextAssets = (artifact.assets ?? []).map((asset, index) =>
      this.normalizeAsset(asset, id, index, now),
    );

    const nextArtifact: ArtifactEntity = {
      ...existing,
      ...artifact,
      id,
      tags: artifact.tags ?? [],
      location: artifact.location ?? {},
      geolocation: { ...artifact.geolocation },
      assets: nextAssets,
      children: existing?.children ?? artifact.children ?? [],
      createdAt: existing?.createdAt ?? artifact.createdAt ?? now,
      updatedAt: now,
    };

    this.store.set(id, nextArtifact);

    return Promise.resolve(this.clone(nextArtifact));
  }

  findAll(): Promise<ArtifactEntity[]> {
    return Promise.resolve(
      [...this.store.values()].map((artifact) => this.clone(artifact)),
    );
  }

  findById(id: string): Promise<ArtifactEntity | null> {
    const artifact = this.store.get(id);
    return Promise.resolve(artifact ? this.clone(artifact) : null);
  }

  deleteById(id: string): Promise<boolean> {
    return Promise.resolve(this.store.delete(id));
  }

  private normalizeAsset(
    asset: ArtifactAssetEntity,
    artifactId: string,
    index: number,
    now: Date,
  ): ArtifactAssetEntity {
    return {
      ...asset,
      id: asset.id || randomUUID(),
      artifactId,
      sortOrder: asset.sortOrder ?? index,
      createdAt: asset.createdAt ?? now,
      updatedAt: now,
    };
  }

  private clone(artifact: ArtifactEntity): ArtifactEntity {
    return {
      ...artifact,
      tags: [...(artifact.tags ?? [])],
      location: artifact.location ? { ...artifact.location } : {},
      geolocation: { ...artifact.geolocation },
      assets: (artifact.assets ?? []).map((asset) => ({ ...asset })),
      children: [...(artifact.children ?? [])],
      metadata: artifact.metadata
        ? { ...artifact.metadata }
        : artifact.metadata,
    };
  }
}
