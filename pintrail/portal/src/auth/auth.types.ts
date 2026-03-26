export type UserRole = 'admin' | 'editor' | 'viewer';

export interface AuthUser {
  id: string;
  email: string;
  role: UserRole;
}

export interface ManagedUser extends AuthUser {
  isActive: boolean;
  createdAt: string;
  updatedAt: string;
}
