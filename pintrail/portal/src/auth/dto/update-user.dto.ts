import {
  IsBoolean,
  IsEmail,
  IsIn,
  IsOptional,
  IsString,
  MinLength,
} from 'class-validator';
import { UserRole } from '../auth.types';

const roles: UserRole[] = ['admin', 'editor', 'viewer'];

export class UpdateUserDto {
  @IsOptional()
  @IsEmail()
  email?: string;

  @IsOptional()
  @IsString()
  @MinLength(8)
  password?: string;

  @IsOptional()
  @IsIn(roles)
  role?: UserRole;

  @IsOptional()
  @IsBoolean()
  isActive?: boolean;
}
