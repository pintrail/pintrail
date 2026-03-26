import {
  Body,
  Controller,
  Delete,
  Get,
  Param,
  Patch,
  Post,
} from '@nestjs/common';
import { ArtifactsService } from './artifacts.service';
import { CreateArtifactDto } from './dto/create-artifact.dto';
import { UpdateArtifactDto } from './dto/update-artifact.dto';

@Controller('api/artifacts')
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

  @Post()
  create(@Body() dto: CreateArtifactDto) {
    return this.artifactsService.create(dto);
  }

  @Patch(':id')
  update(@Param('id') id: string, @Body() dto: UpdateArtifactDto) {
    return this.artifactsService.update(id, dto);
  }

  @Delete(':id')
  remove(@Param('id') id: string) {
    return this.artifactsService.remove(id);
  }
}
