import { AuthUser } from '../auth/auth.types';

declare global {
  namespace Express {
    interface Request {
      cookies?: Record<string, string>;
      user?: AuthUser | null;
    }
  }
}

export {};
