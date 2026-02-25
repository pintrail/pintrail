import { ArtifactEntity } from '../entities/artifact.entity';

export const ARTIFACT_REPOSITORY = Symbol('ARTIFACT_REPOSITORY');

export interface ArtifactRepository {
  save(artifact: ArtifactEntity): Promise<ArtifactEntity>;
  findAll(): Promise<ArtifactEntity[]>;
  findById(id: string): Promise<ArtifactEntity | null>;
  deleteById(id: string): Promise<boolean>;
}
