import {
  Body,
  Controller,
  Delete,
  Get,
  Param,
  Patch,
  Post,
  UploadedFiles,
  UseInterceptors,
} from '@nestjs/common';
import { FilesInterceptor } from '@nestjs/platform-express';
import { Roles } from '../auth/roles.decorator';
import { ArtifactsService } from './artifacts.service';
import { CreateArtifactDto } from './dto/create-artifact.dto';
import { UpdateArtifactDto } from './dto/update-artifact.dto';

@Controller('api/artifacts')
@Roles('viewer')
export class ArtifactsController {
  constructor(private readonly artifactsService: ArtifactsService) {}

  @Get()
  findAll() {
    return this.artifactsService.findAll();
  }

  @Get(':id')
  findOne(@Param('id') id: string) {
    return this.artifactsService.findOne(id);
  }

  @Get(':id/images')
  findImages(@Param('id') id: string) {
    return this.artifactsService.findImages(id);
  }

  @Post()
  @Roles('editor')
  create(@Body() dto: CreateArtifactDto) {
    return this.artifactsService.create(dto);
  }

  @Post(':id/images')
  @UseInterceptors(FilesInterceptor('images'))
  @Roles('editor')
  uploadImages(
    @Param('id') id: string,
    @UploadedFiles() files: Express.Multer.File[],
  ) {
    return this.artifactsService.addImages(id, files ?? []);
  }

  @Delete(':artifactId/images/:imageId')
  @Roles('editor')
  removeImage(
    @Param('artifactId') artifactId: string,
    @Param('imageId') imageId: string,
  ) {
    return this.artifactsService.removeImage(artifactId, imageId);
  }

  @Patch(':id')
  @Roles('editor')
  update(@Param('id') id: string, @Body() dto: UpdateArtifactDto) {
    return this.artifactsService.update(id, dto);
  }

  @Delete(':id')
  @Roles('editor')
  remove(@Param('id') id: string) {
    return this.artifactsService.remove(id);
  }
}
