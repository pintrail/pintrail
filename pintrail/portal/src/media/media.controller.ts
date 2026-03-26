import { Controller, Get, NotFoundException, Param, Res } from '@nestjs/common';
import { Response } from 'express';
import { existsSync } from 'fs';
import { join } from 'path';

@Controller('media')
export class MediaController {
  @Get(':folder/:filename')
  getMedia(
    @Param('folder') folder: string,
    @Param('filename') filename: string,
    @Res() response: Response,
  ) {
    const imageRoot = process.env.IMAGE_STORAGE_ROOT ?? join(process.cwd(), 'data', 'images');
    const safeFolder = folder === 'processed' || folder === 'originals' ? folder : null;

    if (!safeFolder) {
      throw new NotFoundException('Media file was not found.');
    }

    const filePath = join(imageRoot, safeFolder, filename);
    if (!existsSync(filePath)) {
      throw new NotFoundException('Media file was not found.');
    }

    return response.sendFile(filePath);
  }
}
