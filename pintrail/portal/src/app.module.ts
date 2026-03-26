import { Module } from '@nestjs/common';
import { ArtifactsModule } from './artifacts/artifacts.module';
import { FrontendController } from './frontend/frontend.controller';

@Module({
  imports: [ArtifactsModule],
  controllers: [FrontendController],
})
export class AppModule {}
