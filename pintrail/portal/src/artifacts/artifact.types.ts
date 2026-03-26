export interface GeoCoordinates {
  lat: number;
  lng: number;
}

export interface ArtifactImage {
  id: string;
  artifactId: string;
  originalFilename: string;
  status: 'queued' | 'processing' | 'processed' | 'failed';
  url: string | null;
  width: number | null;
  height: number | null;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Artifact {
  id: string;
  name: string;
  desc: string;
  loc: GeoCoordinates | null;
  parentId: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ArtifactDetail extends Artifact {
  children: Artifact[];
  images: ArtifactImage[];
}
