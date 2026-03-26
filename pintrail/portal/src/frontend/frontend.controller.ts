import { Controller, Get, Res } from '@nestjs/common';
import { Response } from 'express';
import { join } from 'path';
import { Public } from '../auth/public.decorator';

@Controller()
@Public()
export class FrontendController {
  @Get()
  getIndex(@Res() response: Response) {
    return response.sendFile(join(process.cwd(), 'src', 'frontend', 'index.html'));
  }
}
