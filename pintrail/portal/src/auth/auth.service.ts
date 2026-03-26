import {
  BadRequestException,
  Injectable,
  OnModuleInit,
  UnauthorizedException,
} from '@nestjs/common';
import { InjectRepository } from '@nestjs/typeorm';
import { randomBytes, scrypt as scryptCallback, timingSafeEqual, createHash } from 'crypto';
import { promisify } from 'util';
import { Repository } from 'typeorm';
import { AuthUser, ManagedUser, UserRole } from './auth.types';
import { CreateUserDto } from './dto/create-user.dto';
import { UpdateUserDto } from './dto/update-user.dto';
import { UserSessionEntity } from './user-session.entity';
import { UserEntity } from './user.entity';

const scrypt = promisify(scryptCallback);

@Injectable()
export class AuthService implements OnModuleInit {
  private readonly sessionDurationMs =
    Number(process.env.AUTH_SESSION_TTL_HOURS ?? 24) * 60 * 60 * 1000;

  constructor(
    @InjectRepository(UserEntity)
    private readonly usersRepository: Repository<UserEntity>,
    @InjectRepository(UserSessionEntity)
    private readonly sessionsRepository: Repository<UserSessionEntity>,
  ) {}

  async onModuleInit() {
    await this.ensureAdminUser();
  }

  async login(
    email: string,
    password: string,
  ): Promise<{ token: string; user: AuthUser; expiresAt: Date }> {
    const normalizedEmail = this.normalizeEmail(email);
    const user = await this.usersRepository.findOneBy({ email: normalizedEmail });

    if (!user || !user.isActive) {
      throw new UnauthorizedException('Invalid email or password.');
    }

    const passwordMatches = await this.verifyPassword(password, user.passwordHash);
    if (!passwordMatches) {
      throw new UnauthorizedException('Invalid email or password.');
    }

    const token = randomBytes(32).toString('hex');
    const expiresAt = new Date(Date.now() + this.sessionDurationMs);
    const session = this.sessionsRepository.create({
      userId: user.id,
      tokenHash: this.hashToken(token),
      expiresAt,
    });

    await this.sessionsRepository.save(session);

    return {
      token,
      user: this.toAuthUser(user),
      expiresAt,
    };
  }

  async logout(sessionToken: string | null | undefined) {
    if (!sessionToken) {
      return;
    }

    await this.sessionsRepository.delete({
      tokenHash: this.hashToken(sessionToken),
    });
  }

  async authenticate(sessionToken: string | null | undefined): Promise<AuthUser | null> {
    if (!sessionToken) {
      return null;
    }

    const session = await this.sessionsRepository.findOne({
      where: { tokenHash: this.hashToken(sessionToken) },
      relations: { user: true },
    });

    if (!session) {
      return null;
    }

    if (session.expiresAt.getTime() <= Date.now() || !session.user.isActive) {
      await this.sessionsRepository.delete(session.id);
      return null;
    }

    return this.toAuthUser(session.user);
  }

  async findAllUsers(): Promise<ManagedUser[]> {
    const users = await this.usersRepository.find({
      order: { createdAt: 'ASC' },
    });

    return users.map(user => this.toManagedUser(user));
  }

  async createUser(dto: CreateUserDto): Promise<ManagedUser> {
    const email = this.normalizeEmail(dto.email);
    const existingUser = await this.usersRepository.findOneBy({ email });
    if (existingUser) {
      throw new BadRequestException(`A user with email ${email} already exists.`);
    }

    const user = this.usersRepository.create({
      email,
      passwordHash: await this.hashPassword(dto.password),
      role: dto.role ?? 'viewer',
      isActive: true,
    });

    const savedUser = await this.usersRepository.save(user);
    return this.toManagedUser(savedUser);
  }

  async updateUser(id: string, dto: UpdateUserDto): Promise<ManagedUser> {
    const user = await this.usersRepository.findOneBy({ id });
    if (!user) {
      throw new BadRequestException(`User ${id} was not found.`);
    }

    if (dto.email !== undefined) {
      const normalizedEmail = this.normalizeEmail(dto.email);
      const existingUser = await this.usersRepository.findOneBy({ email: normalizedEmail });
      if (existingUser && existingUser.id !== id) {
        throw new BadRequestException(`A user with email ${normalizedEmail} already exists.`);
      }
      user.email = normalizedEmail;
    }

    if (dto.password !== undefined) {
      user.passwordHash = await this.hashPassword(dto.password);
    }

    if (dto.role !== undefined) {
      user.role = dto.role;
    }

    if (dto.isActive !== undefined) {
      user.isActive = dto.isActive;
      if (!dto.isActive) {
        await this.sessionsRepository.delete({ userId: id });
      }
    }

    const savedUser = await this.usersRepository.save(user);
    return this.toManagedUser(savedUser);
  }

  private async ensureAdminUser() {
    const adminEmail = process.env.AUTH_ADMIN_EMAIL
      ? this.normalizeEmail(process.env.AUTH_ADMIN_EMAIL)
      : undefined;
    const adminPassword = process.env.AUTH_ADMIN_PASSWORD?.trim();

    if (!adminEmail || !adminPassword) {
      return;
    }

    const existingUser = await this.usersRepository.findOneBy({ email: adminEmail });
    if (existingUser) {
      if (existingUser.role !== 'admin' || !existingUser.isActive) {
        existingUser.role = 'admin';
        existingUser.isActive = true;
        await this.usersRepository.save(existingUser);
      }
      return;
    }

    const user = this.usersRepository.create({
      email: adminEmail,
      passwordHash: await this.hashPassword(adminPassword),
      role: 'admin',
      isActive: true,
    });
    await this.usersRepository.save(user);
  }

  async hashPassword(password: string): Promise<string> {
    const salt = randomBytes(16).toString('hex');
    const derivedKey = (await scrypt(password, salt, 64)) as Buffer;
    return `${salt}:${derivedKey.toString('hex')}`;
  }

  private async verifyPassword(password: string, storedHash: string): Promise<boolean> {
    const [salt, key] = storedHash.split(':');
    if (!salt || !key) {
      return false;
    }

    const derivedKey = (await scrypt(password, salt, 64)) as Buffer;
    const storedKeyBuffer = Buffer.from(key, 'hex');
    if (derivedKey.length !== storedKeyBuffer.length) {
      return false;
    }

    return timingSafeEqual(derivedKey, storedKeyBuffer);
  }

  private hashToken(token: string): string {
    return createHash('sha256').update(token).digest('hex');
  }

  private normalizeEmail(email: string): string {
    return email.trim().toLowerCase();
  }

  private toAuthUser(user: UserEntity): AuthUser {
    return {
      id: user.id,
      email: user.email,
      role: user.role as UserRole,
    };
  }

  private toManagedUser(user: UserEntity): ManagedUser {
    return {
      ...this.toAuthUser(user),
      isActive: user.isActive,
      createdAt: user.createdAt.toISOString(),
      updatedAt: user.updatedAt.toISOString(),
    };
  }
}
