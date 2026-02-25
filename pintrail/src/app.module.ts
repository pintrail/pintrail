import { Module } from '@nestjs/common';
import { AppController } from './app.controller';
import { AppService } from './app.service';
import { ArtifactsModule } from './artifacts/artifacts.module';

@Module({
  imports: [ArtifactsModule],
  controllers: [AppController],
  providers: [AppService],
})
export class AppModule {}
