from typing import List
from fastapi import FastAPI, Depends, APIRouter
from sqlmodel import select
from sqlmodel.ext.asyncio.session import AsyncSession
from .user_orm import User
from .user_db import get_session, init_db, redis_client
from .dto.user_schema import UserCreate, UserRead
from fastapi import HTTPException, Response, status
from sqlalchemy.exc import IntegrityError

router = APIRouter(tags=["users"])


@router.on_event("startup")
async def on_startup():
    await init_db()
    await redis_client.ping() # type: ignore


@router.get("/healthz/liveness")
async def liveness():
    return {"ok": True}


@router.get("/healthz/readiness")
async def readiness():
    try:
        await redis_client.ping() # type: ignore
        return {"ok": True}
    except Exception:
        return {"ok": False}


# @app.get("/api/users")
# async def list_users(session: AsyncSession = Depends(get_session)):
#     result = await session.exec(select(User))
#     return result.scalars().all()


@router.get("/api/users", response_model=list[UserRead])
async def list_users(session: AsyncSession = Depends(get_session)):
    result = await session.exec(select(User).order_by(User.id))
    users = result.all()          # ScalarResult → list[User]
    return users

# Dangerous route - no checking.
@router.post("/api/users")
async def create_user(user: User, session: AsyncSession = Depends(get_session)):
    session.add(user)
    await session.commit()
    await session.refresh(user)
    return user


@router.post("/api/users/create", response_model=UserRead, status_code=status.HTTP_201_CREATED)
async def api_create_user(payload: UserCreate, response: Response, session: AsyncSession = Depends(get_session)) -> UserRead:
    # Normalize inputs
    username = payload.username.strip()
    email = payload.email.strip().lower()

    # Preflight uniqueness check for fast, friendly errors
    stmt = select(User).where((User.username == username) | (User.email == email))
    existing = (await session.exec(stmt)).first()
    if existing:
        conflict_field = "username" if existing.username == username else "email"
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=f"{conflict_field} already exists",
        )

    # Create and persist
    user = User(username=username, email=email)
    session.add(user)
    try:
        await session.commit()  # flush + commit, DB generates id; unique constraints enforced here
    except IntegrityError:
        # Handles race conditions where another row slipped in after our preflight check
        await session.rollback()
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="username or email already exists",
        )

    await session.refresh(user)  # populate id, public_id, created_at

    # Set Location header to the new resource
    response.headers["Location"] = f"/api/users/{user.public_id}"

    return UserRead.model_validate(user)
