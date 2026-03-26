import { IsEmail, IsIn, IsOptional, IsString, MinLength } from 'class-validator';
import { UserRole } from '../auth.types';

const roles: UserRole[] = ['admin', 'editor', 'viewer'];

export class CreateUserDto {
  @IsEmail()
  email!: string;

  @IsString()
  @MinLength(8)
  password!: string;

  @IsOptional()
  @IsIn(roles)
  role?: UserRole;
}
