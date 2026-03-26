import {
  BadRequestException,
  Injectable,
  NotFoundException,
} from '@nestjs/common';
import { randomUUID } from 'crypto';
import { Artifact, ArtifactDetail, GeoCoordinates } from './artifact.types';
import { CreateArtifactDto } from './dto/create-artifact.dto';
import { UpdateArtifactDto } from './dto/update-artifact.dto';

@Injectable()
export class ArtifactsService {
  private readonly artifacts = new Map<string, Artifact>();

  create(dto: CreateArtifactDto): Artifact {
    const parentId = dto.parentId?.trim() || null;
    const parentArtifact = parentId ? this.artifacts.get(parentId) : null;

    if (parentId && !parentArtifact) {
      throw new BadRequestException(`Parent artifact ${parentId} does not exist.`);
    }

    const now = new Date().toISOString();
    const artifact: Artifact = {
      id: randomUUID(),
      name: '',
      desc: '',
      loc: parentArtifact?.loc ? { ...parentArtifact.loc } : null,
      parentId,
      createdAt: now,
      updatedAt: now,
    };

    this.artifacts.set(artifact.id, artifact);
    return artifact;
  }

  findAll(): Artifact[] {
    return [...this.artifacts.values()].sort((left, right) => {
      return left.createdAt.localeCompare(right.createdAt);
    });
  }

  findOne(id: string): ArtifactDetail {
    const artifact = this.requireArtifact(id);
    return {
      ...artifact,
      children: this.findChildren(id),
    };
  }

  update(id: string, dto: UpdateArtifactDto): ArtifactDetail {
    const artifact = this.requireArtifact(id);

    if (dto.parentId !== undefined) {
      const nextParentId = dto.parentId?.trim() || null;

      if (nextParentId === id) {
        throw new BadRequestException('An artifact cannot be its own parent.');
      }

      if (nextParentId && !this.artifacts.has(nextParentId)) {
        throw new BadRequestException(`Parent artifact ${nextParentId} does not exist.`);
      }

      if (nextParentId && this.isDescendant(nextParentId, id)) {
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
      artifact.loc = null;
    } else if (dto.lat !== undefined || dto.lng !== undefined) {
      artifact.loc = this.normalizeCoordinates(dto.lat, dto.lng);
    }

    artifact.updatedAt = new Date().toISOString();
    this.artifacts.set(id, artifact);

    return this.findOne(id);
  }

  remove(id: string): { deletedIds: string[] } {
    this.requireArtifact(id);

    const deletedIds: string[] = [];
    this.collectDescendantsForDeletion(id, deletedIds);

    for (const artifactId of deletedIds) {
      this.artifacts.delete(artifactId);
    }

    return { deletedIds };
  }

  private requireArtifact(id: string): Artifact {
    const artifact = this.artifacts.get(id);
    if (!artifact) {
      throw new NotFoundException(`Artifact ${id} was not found.`);
    }

    return artifact;
  }

  private findChildren(parentId: string): Artifact[] {
    return this.findAll().filter(artifact => artifact.parentId === parentId);
  }

  private isDescendant(candidateId: string, ancestorId: string): boolean {
    let currentId: string | null | undefined = candidateId;

    while (currentId) {
      if (currentId === ancestorId) {
        return true;
      }

      currentId = this.artifacts.get(currentId)?.parentId;
    }

    return false;
  }

  private collectDescendantsForDeletion(id: string, deletedIds: string[]) {
    const childIds = this.findChildren(id).map(artifact => artifact.id);

    for (const childId of childIds) {
      this.collectDescendantsForDeletion(childId, deletedIds);
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
}
