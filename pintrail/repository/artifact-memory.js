// This is an in-memory implementation of the artifact repository.

const example = Artifact({
  id: 1,
  name: "Basket",
  description: "A small brown basket",
  tags: ["small", "brown"],
  createdAt: new Date(),
  updatedAt: new Date(),
});

const Artifact = (data) => ({
  id: () => data.id,
  name: () => data.name,
  description: () => data.description,
  tags: () => data.tags,
  createdAt: () => data.createdAt,
  updatedAt: () => data.updatedAt,
  x: () => data.x,
  y: () => data.y,
  pid: () => data.pid,
});

const ArtifactRepository = () => {
  const artifacts = [];

  return {
    make: (data) => Artifact(data),
    id: (id) => artifacts.find((artifact) => artifact.id() === id),
    all: () => artifacts,
  };
};

const MakeArtifactRepository = () => ArtifactRepository();

export default MakeArtifactRepository;
