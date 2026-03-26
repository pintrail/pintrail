import 'reflect-metadata';
import { mkdirSync } from 'fs';
import { join } from 'path';
import { parse as parseCookie } from 'cookie';
import { ValidationPipe } from '@nestjs/common';
import { NestFactory } from '@nestjs/core';
import { NestExpressApplication } from '@nestjs/platform-express';
import { NextFunction, Request, Response } from 'express';
import { AppModule } from './app.module';

async function bootstrap() {
  const app = await NestFactory.create<NestExpressApplication>(AppModule);
  const imageRoot = process.env.IMAGE_STORAGE_ROOT ?? join(process.cwd(), 'data', 'images');

  mkdirSync(imageRoot, { recursive: true });

  app.use((request: Request, _response: Response, next: NextFunction) => {
    request.cookies = parseCookie(request.headers.cookie ?? '');
    next();
  });
  app.useStaticAssets(join(process.cwd(), 'src', 'frontend'));
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
