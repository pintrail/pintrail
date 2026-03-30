import hashlib
import hmac
import os
import secrets
import uuid
from datetime import datetime, timedelta, timezone

from sqlalchemy import select
from sqlmodel.ext.asyncio.session import AsyncSession

from app.config import settings
from app.models.user import User, UserRole, UserSession


def _hash_password(password: str) -> str:
    salt = os.urandom(16)
    dk = hashlib.scrypt(password.encode(), salt=salt, n=16384, r=8, p=1)
    return f"{salt.hex()}:{dk.hex()}"


def _verify_password(password: str, stored: str) -> bool:
    try:
        salt_hex, dk_hex = stored.split(":", 1)
        salt = bytes.fromhex(salt_hex)
        dk_expected = bytes.fromhex(dk_hex)
        dk_actual = hashlib.scrypt(password.encode(), salt=salt, n=16384, r=8, p=1)
        return hmac.compare_digest(dk_actual, dk_expected)
    except Exception:
        return False


def _hash_token(token: bytes) -> str:
    return hashlib.sha256(token).hexdigest()


async def create_admin_if_needed(db: AsyncSession) -> None:
    if not settings.auth_admin_email or not settings.auth_admin_password:
        return
    result = await db.execute(select(User).where(User.email == settings.auth_admin_email))
    existing = result.scalars().first()
    if existing:
        return
    admin = User(
        email=settings.auth_admin_email,
        password_hash=_hash_password(settings.auth_admin_password),
        role=UserRole.admin,
        is_active=True,
    )
    db.add(admin)
    await db.commit()
    print(f"[auth] Created admin user: {settings.auth_admin_email}")


async def login(db: AsyncSession, email: str, password: str) -> str | None:
    """Validate credentials and return a session token, or None on failure."""
    result = await db.execute(select(User).where(User.email == email))
    user = result.scalars().first()
    if not user or not user.is_active:
        return None
    if not _verify_password(password, user.password_hash):
        return None

    token = secrets.token_bytes(32)
    token_hex = token.hex()
    token_hash = _hash_token(token)
    expires_at = datetime.now(timezone.utc) + timedelta(hours=settings.auth_session_ttl_hours)

    session = UserSession(
        user_id=user.id,
        token_hash=token_hash,
        expires_at=expires_at,
    )
    db.add(session)
    await db.commit()
    return token_hex


async def get_user_from_token(db: AsyncSession, token_hex: str) -> User | None:
    """Resolve a session token to its User, or None if invalid/expired."""
    try:
        token = bytes.fromhex(token_hex)
    except ValueError:
        return None
    token_hash = _hash_token(token)
    now = datetime.now(timezone.utc)

    result = await db.execute(
        select(UserSession).where(
            UserSession.token_hash == token_hash,
            UserSession.expires_at > now,
        )
    )
    session = result.scalars().first()
    if not session:
        return None

    result = await db.execute(select(User).where(User.id == session.user_id))
    user = result.scalars().first()
    if not user or not user.is_active:
        return None
    return user


async def logout(db: AsyncSession, token_hex: str) -> None:
    try:
        token = bytes.fromhex(token_hex)
    except ValueError:
        return
    token_hash = _hash_token(token)
    result = await db.execute(
        select(UserSession).where(UserSession.token_hash == token_hash)
    )
    session = result.scalars().first()
    if session:
        await db.delete(session)
        await db.commit()


async def create_user(
    db: AsyncSession,
    email: str,
    password: str,
    role: UserRole = UserRole.viewer,
) -> User:
    user = User(
        email=email,
        password_hash=_hash_password(password),
        role=role,
        is_active=True,
    )
    db.add(user)
    await db.commit()
    await db.refresh(user)
    return user


async def update_user(
    db: AsyncSession,
    user_id: uuid.UUID,
    *,
    role: UserRole | None = None,
    is_active: bool | None = None,
    password: str | None = None,
) -> User | None:
    result = await db.execute(select(User).where(User.id == user_id))
    user = result.scalars().first()
    if not user:
        return None
    if role is not None:
        user.role = role
    if is_active is not None:
        user.is_active = is_active
    if password is not None:
        user.password_hash = _hash_password(password)
    user.updated_at = datetime.now(timezone.utc)
    db.add(user)
    await db.commit()
    await db.refresh(user)
    return user


async def list_users(db: AsyncSession) -> list[User]:
    result = await db.execute(select(User).order_by(User.created_at))
    return list(result.scalars().all())
