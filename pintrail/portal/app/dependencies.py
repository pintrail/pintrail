from typing import Optional

from fastapi import Cookie, Depends, HTTPException, Request, status
from sqlmodel.ext.asyncio.session import AsyncSession

from app.database import get_session
from app.models.user import User, UserRole, ROLE_RANK
from app.services.auth import get_user_from_token

COOKIE_NAME = "pintrail_session"


async def get_optional_user(
    request: Request,
    db: AsyncSession = Depends(get_session),
) -> Optional[User]:
    token = request.cookies.get(COOKIE_NAME)
    if not token:
        return None
    return await get_user_from_token(db, token)


async def require_user(
    user: Optional[User] = Depends(get_optional_user),
) -> User:
    if not user:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Not authenticated")
    return user


def require_role(minimum_role: UserRole):
    async def _check(user: User = Depends(require_user)) -> User:
        if ROLE_RANK[user.role] < ROLE_RANK[minimum_role]:
            raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Insufficient permissions")
        return user
    return _check


require_viewer = require_role(UserRole.viewer)
require_editor = require_role(UserRole.editor)
require_admin = require_role(UserRole.admin)
