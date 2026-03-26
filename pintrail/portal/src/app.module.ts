import { Module } from '@nestjs/common';
import { TypeOrmModule } from '@nestjs/typeorm';
import { ArtifactsModule } from './artifacts/artifacts.module';
import { FrontendController } from './frontend/frontend.controller';

@Module({
  imports: [
    TypeOrmModule.forRoot({
      type: 'postgres',
      host: process.env.DB_HOST ?? 'localhost',
      port: Number(process.env.DB_PORT ?? 5432),
      username: process.env.DB_USER ?? 'pintrail',
      password: process.env.DB_PASSWORD ?? 'pintrail',
      database: process.env.DB_NAME ?? 'pintrail',
      autoLoadEntities: true,
      synchronize: true,
    }),
    ArtifactsModule,
  ],
  controllers: [FrontendController],
})
export class AppModule {}
