export interface GeoCoordinates {
  lat: number;
  lng: number;
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
}
