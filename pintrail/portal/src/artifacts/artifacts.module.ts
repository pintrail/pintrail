import { Module } from '@nestjs/common';
import { TypeOrmModule } from '@nestjs/typeorm';
import { ArtifactImageEntity } from './artifact-image.entity';
import { ArtifactsController } from './artifacts.controller';
import { ArtifactEntity } from './artifact.entity';
import { ArtifactsService } from './artifacts.service';

@Module({
  imports: [TypeOrmModule.forFeature([ArtifactEntity, ArtifactImageEntity])],
  controllers: [ArtifactsController],
  providers: [ArtifactsService],
  exports: [ArtifactsService],
})
export class ArtifactsModule {}
