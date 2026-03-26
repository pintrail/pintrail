import {
  Body,
  Controller,
  Get,
  HttpCode,
  Param,
  Patch,
  Post,
  Req,
  Res,
} from '@nestjs/common';
import { Request, Response } from 'express';
import { AuthService } from './auth.service';
import { CurrentUser } from './current-user.decorator';
import { CreateUserDto } from './dto/create-user.dto';
import { LoginDto } from './dto/login.dto';
import { UpdateUserDto } from './dto/update-user.dto';
import { Public } from './public.decorator';
import { Roles } from './roles.decorator';
import { AuthUser } from './auth.types';

@Controller('api/auth')
export class AuthController {
  constructor(private readonly authService: AuthService) {}

  @Public()
  @HttpCode(200)
  @Post('login')
  async login(@Body() dto: LoginDto, @Res({ passthrough: true }) response: Response) {
    const { token, user, expiresAt } = await this.authService.login(dto.email, dto.password);

    response.cookie('pintrail_session', token, {
      httpOnly: true,
      sameSite: 'lax',
      secure: process.env.NODE_ENV === 'production',
      expires: expiresAt,
      path: '/',
    });

    return { user };
  }

  @Get('me')
  me(@CurrentUser() user: AuthUser | undefined) {
    return { user: user ?? null };
  }

  @Get('users')
  @Roles('admin')
  async findUsers() {
    return { users: await this.authService.findAllUsers() };
  }

  @Post('users')
  @Roles('admin')
  async createUser(@Body() dto: CreateUserDto) {
    return { user: await this.authService.createUser(dto) };
  }

  @Patch('users/:id')
  @Roles('admin')
  async updateUser(@Param('id') id: string, @Body() dto: UpdateUserDto) {
    return { user: await this.authService.updateUser(id, dto) };
  }

  @HttpCode(200)
  @Post('logout')
  async logout(@Req() request: Request, @Res({ passthrough: true }) response: Response) {
    const sessionToken = request.cookies?.pintrail_session ?? null;
    await this.authService.logout(sessionToken);
    response.clearCookie('pintrail_session', {
      path: '/',
      sameSite: 'lax',
      secure: process.env.NODE_ENV === 'production',
    });
    return { success: true };
  }
}
