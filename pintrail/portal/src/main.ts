import 'reflect-metadata';
import { mkdirSync } from 'fs';
import { join } from 'path';
import { ValidationPipe } from '@nestjs/common';
import { NestFactory } from '@nestjs/core';
import { NestExpressApplication } from '@nestjs/platform-express';
import { AppModule } from './app.module';

async function bootstrap() {
  const app = await NestFactory.create<NestExpressApplication>(AppModule);
  const imageRoot = process.env.IMAGE_STORAGE_ROOT ?? join(process.cwd(), 'data', 'images');

  mkdirSync(imageRoot, { recursive: true });

  app.useStaticAssets(join(process.cwd(), 'src', 'frontend'));
  app.useStaticAssets(imageRoot, { prefix: '/media/' });
  app.useGlobalPipes(
    new ValidationPipe({
      whitelist: true,
      transform: true,
      forbidNonWhitelisted: true,
    }),
  );

  const port = Number(process.env.PORT ?? 3000);
  await app.listen(port);
  console.log(`Pintrail is running at http://localhost:${port}`);
}

void bootstrap();
