import { Test, TestingModule } from '@nestjs/testing';
import { ArtifactsController } from './artifacts.controller';
import { ArtifactsService } from './artifacts.service';

describe('ArtifactsController', () => {
  let controller: ArtifactsController;

  const serviceMock = {
    create: jest.fn(),
    findAll: jest.fn(),
    findOne: jest.fn(),
    update: jest.fn(),
    remove: jest.fn(),
  };

  beforeEach(async () => {
    jest.clearAllMocks();

    const module: TestingModule = await Test.createTestingModule({
      controllers: [ArtifactsController],
      providers: [
        {
          provide: ArtifactsService,
          useValue: serviceMock,
        },
      ],
    }).compile();

    controller = module.get<ArtifactsController>(ArtifactsController);
  });

  it('creates an artifact', async () => {
    serviceMock.create.mockResolvedValue({
      id: 'artifact-1',
      name: 'Artifact One',
    });

    const result = await controller.create({ name: 'Artifact One' });

    expect(serviceMock.create.mock.calls[0]).toEqual([
      { name: 'Artifact One' },
    ]);
    expect(result).toEqual({ id: 'artifact-1', name: 'Artifact One' });
  });

  it('gets all artifacts', async () => {
    serviceMock.findAll.mockResolvedValue([{ id: 'artifact-1' }]);

    const result = await controller.findAll();

    expect(serviceMock.findAll.mock.calls).toHaveLength(1);
    expect(result).toEqual([{ id: 'artifact-1' }]);
  });

  it('gets one artifact by id', async () => {
    serviceMock.findOne.mockResolvedValue({ id: 'artifact-1' });

    const result = await controller.findOne('artifact-1');

    expect(serviceMock.findOne.mock.calls[0]).toEqual(['artifact-1']);
    expect(result).toEqual({ id: 'artifact-1' });
  });

  it('updates an artifact', async () => {
    serviceMock.update.mockResolvedValue({ id: 'artifact-1', name: 'Updated' });

    const result = await controller.update('artifact-1', { name: 'Updated' });

    expect(serviceMock.update.mock.calls[0]).toEqual([
      'artifact-1',
      { name: 'Updated' },
    ]);
    expect(result).toEqual({ id: 'artifact-1', name: 'Updated' });
  });

  it('deletes an artifact', async () => {
    serviceMock.remove.mockResolvedValue(undefined);

    await expect(controller.remove('artifact-1')).resolves.toBeUndefined();
    expect(serviceMock.remove.mock.calls[0]).toEqual(['artifact-1']);
  });
});
