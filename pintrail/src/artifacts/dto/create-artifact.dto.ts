import { ArtifactAssetType, ArtifactKind } from '../entities/artifact.entity';

export class ArtifactLocationDto {
  name?: string;
  addressLine1?: string;
  addressLine2?: string;
  city?: string;
  stateProvince?: string;
  postalCode?: string;
  country?: string;
  building?: string;
  floor?: string;
  room?: string;
  shelf?: string;
  latitude?: number;
  longitude?: number;
  notes?: string;
}

export class CreateArtifactAssetDto {
  type: ArtifactAssetType;
  title?: string;
  description?: string;
  sortOrder?: number;
  textContent?: string;
  textFormat?: 'plain' | 'markdown' | 'html';
  url?: string;
  mimeType?: string;
  durationSeconds?: number;
  width?: number;
  height?: number;
  sizeBytes?: string;
  linkedArtifactId?: string;
  relationship?: string;
  metadata?: Record<string, unknown>;
}

export class CreateArtifactDto {
  kind?: ArtifactKind;
  name: string;
  description?: string;
  tags?: string[];
  location?: ArtifactLocationDto;
  parentArtifactId?: string | null;
  assets?: CreateArtifactAssetDto[];
  isActive?: boolean;
  metadata?: Record<string, unknown>;
}
