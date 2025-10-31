
from typing import List
from fastapi import FastAPI, Depends, APIRouter
from sqlmodel import select
from sqlmodel.ext.asyncio.session import AsyncSession
from .artifact_orm import Artifacts
from .artifact_db import get_session, init_db, redis_client
from .dto.user_schema import UserCreate, UserRead
from fastapi import HTTPException, Response, status
from sqlalchemy.exc import IntegrityError

router = APIRouter(tags=["artifacts"])


@router.on_event("startup")
async def on_startup():
    await init_db()
    await redis_client.ping() # type: ignore


@router.get("/healthz/liveness")
async def liveness():
    return {"ok": True}