import { Module } from '@nestjs/common';
import { ArtifactsController } from './artifacts.controller';
import { ArtifactsService } from './artifacts.service';
import { InMemoryArtifactRepository } from './repositories/in-memory-artifact.repository';
import { ARTIFACT_REPOSITORY } from './repositories/artifact.repository';

@Module({
  controllers: [ArtifactsController],
  providers: [
    ArtifactsService,
    InMemoryArtifactRepository,
    {
      provide: ARTIFACT_REPOSITORY,
      useExisting: InMemoryArtifactRepository,
    },
  ],
  exports: [ArtifactsService],
})
export class ArtifactsModule {}
