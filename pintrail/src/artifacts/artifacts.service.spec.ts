import { NotFoundException } from '@nestjs/common';
import { Test, TestingModule } from '@nestjs/testing';
import { ArtifactsService } from './artifacts.service';
import { ArtifactAssetType, ArtifactKind } from './entities/artifact.entity';
import {
  ARTIFACT_REPOSITORY,
  ArtifactRepository,
} from './repositories/artifact.repository';

describe('ArtifactsService', () => {
  let service: ArtifactsService;

  const repositoryMock: jest.Mocked<ArtifactRepository> = {
    save: jest.fn(),
    findAll: jest.fn(),
    findById: jest.fn(),
    deleteById: jest.fn(),
  };

  beforeEach(async () => {
    jest.clearAllMocks();

    const module: TestingModule = await Test.createTestingModule({
      providers: [
        ArtifactsService,
        {
          provide: ARTIFACT_REPOSITORY,
          useValue: repositoryMock,
        },
      ],
    }).compile();

    service = module.get<ArtifactsService>(ArtifactsService);
  });

  it('creates an artifact', async () => {
    repositoryMock.save.mockImplementation((artifact) =>
      Promise.resolve({
        ...artifact,
        id: 'artifact-1',
        createdAt: new Date('2026-02-25T00:00:00.000Z'),
        updatedAt: new Date('2026-02-25T00:00:00.000Z'),
      }),
    );

    const created = await service.create({
      name: 'North Gallery',
      kind: ArtifactKind.ROOM,
      tags: ['gallery', 'floor-1'],
      location: {
        building: 'Main Museum',
        floor: '1',
        room: 'North Gallery',
      },
      assets: [
        {
          type: ArtifactAssetType.TEXT,
          textContent: 'Primary display room',
          textFormat: 'plain',
        },
      ],
    });

    expect(repositoryMock.save.mock.calls).toHaveLength(1);
    expect(created.id).toBe('artifact-1');
    expect(created.name).toBe('North Gallery');
    expect(created.assets[0]?.type).toBe(ArtifactAssetType.TEXT);
  });

  it('returns all artifacts', async () => {
    repositoryMock.findAll.mockResolvedValue([
      {
        id: 'artifact-1',
        kind: ArtifactKind.ITEM,
        name: 'Vase',
        description: null,
        tags: [],
        location: {},
        children: [],
        assets: [],
        isActive: true,
        metadata: null,
        createdAt: new Date(),
        updatedAt: new Date(),
      } as never,
    ]);

    const result = await service.findAll();

    expect(result).toHaveLength(1);
    expect(repositoryMock.findAll.mock.calls).toHaveLength(1);
  });

  it('throws on missing artifact lookup', async () => {
    repositoryMock.findById.mockResolvedValue(null);

    await expect(service.findOne('missing-id')).rejects.toBeInstanceOf(
      NotFoundException,
    );
  });

  it('updates an artifact', async () => {
    repositoryMock.findById.mockResolvedValue({
      id: 'artifact-1',
      kind: ArtifactKind.ITEM,
      name: 'Old Name',
      description: null,
      tags: ['old'],
      location: {},
      parentArtifactId: null,
      children: [],
      assets: [],
      isActive: true,
      metadata: null,
      createdAt: new Date('2026-02-24T00:00:00.000Z'),
      updatedAt: new Date('2026-02-24T00:00:00.000Z'),
    } as never);
    repositoryMock.save.mockImplementation((artifact) =>
      Promise.resolve(artifact),
    );

    const updated = await service.update('artifact-1', {
      name: 'New Name',
      tags: ['updated'],
    });

    expect(updated.name).toBe('New Name');
    expect(updated.tags).toEqual(['updated']);
    expect(repositoryMock.save.mock.calls).toHaveLength(1);
  });

  it('throws on delete when artifact does not exist', async () => {
    repositoryMock.deleteById.mockResolvedValue(false);

    await expect(service.remove('missing-id')).rejects.toBeInstanceOf(
      NotFoundException,
    );
  });
});
