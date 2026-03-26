import {
  CanActivate,
  ExecutionContext,
  ForbiddenException,
  Injectable,
  UnauthorizedException,
} from '@nestjs/common';
import { Reflector } from '@nestjs/core';
import { parse as parseCookie } from 'cookie';
import { AuthService } from './auth.service';
import { ROLES_KEY } from './roles.decorator';
import { IS_PUBLIC_KEY } from './public.decorator';
import { UserRole } from './auth.types';

const roleRank: Record<UserRole, number> = {
  viewer: 1,
  editor: 2,
  admin: 3,
};

@Injectable()
export class AuthGuard implements CanActivate {
  constructor(
    private readonly reflector: Reflector,
    private readonly authService: AuthService,
  ) {}

  async canActivate(context: ExecutionContext): Promise<boolean> {
    const isPublic = this.reflector.getAllAndOverride<boolean>(IS_PUBLIC_KEY, [
      context.getHandler(),
      context.getClass(),
    ]);

    const request = context.switchToHttp().getRequest();
    const response = context.switchToHttp().getResponse();
    const sessionToken = this.extractSessionToken(request.headers.cookie);
    const user = await this.authService.authenticate(sessionToken);

    request.user = user;

    if (!user) {
      if (isPublic) {
        return true;
      }

      throw new UnauthorizedException('Authentication is required.');
    }

    response.locals.user = user;

    const requiredRoles = this.reflector.getAllAndOverride<UserRole[]>(ROLES_KEY, [
      context.getHandler(),
      context.getClass(),
    ]);

    if (!requiredRoles?.length) {
      return true;
    }

    const isAuthorized = requiredRoles.some(role => roleRank[user.role] >= roleRank[role]);
    if (!isAuthorized) {
      throw new ForbiddenException('You are not authorized to perform this action.');
    }

    return true;
  }

  private extractSessionToken(cookieHeader?: string): string | null {
    if (!cookieHeader) {
      return null;
    }

    const cookies = parseCookie(cookieHeader);
    return cookies.pintrail_session ?? null;
  }
}
