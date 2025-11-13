
from typing import List
from fastapi import FastAPI, Depends, APIRouter
from sqlmodel import select
from sqlmodel.ext.asyncio.session import AsyncSession
from db import Artifacts
from artifact_db import get_session, init_db, redis_client
from dto.artifact_schema import ArtifactCreate, ArtifactResponse
from artifact_service import create_artifact_service

router = APIRouter(tags=["artifacts"])

# @router.on_event("startup")
# async def on_startup():
#     await init_db()
#     await redis_client.ping() # type: ignore


@router.get("/health")
async def liveness():
    return {"health": "ok"}

@router.post("/create", response_model=ArtifactResponse)
async def create_artifact_controller(artifact_data: ArtifactCreate, db: AsyncSession = Depends(get_session)):
    return await create_artifact_service(artifact_data, db)