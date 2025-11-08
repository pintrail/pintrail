
from typing import List
from fastapi import FastAPI, Depends, APIRouter
from sqlmodel import select
from sqlmodel.ext.asyncio.session import AsyncSession
from db import Artifacts
from .artifact_db import get_session, init_db, redis_client

router = APIRouter(tags=["artifacts"])

@router.on_event("startup")
async def on_startup():
    await init_db()
    await redis_client.ping() # type: ignore


@router.get("/health")
async def liveness():
    return {"ok": True}

@router.post("/create")
async def create_artifact_controller():
    return 