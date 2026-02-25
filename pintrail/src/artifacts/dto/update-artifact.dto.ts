import {
  CreateArtifactAssetDto,
  ArtifactLocationDto,
} from './create-artifact.dto';
import { ArtifactKind } from '../entities/artifact.entity';

export class UpdateArtifactDto {
  kind?: ArtifactKind;
  name?: string;
  description?: string | null;
  tags?: string[];
  location?: ArtifactLocationDto;
  parentArtifactId?: string | null;
  assets?: CreateArtifactAssetDto[];
  isActive?: boolean;
  metadata?: Record<string, unknown> | null;
}
