
from typing import List
from fastapi import FastAPI, Depends, APIRouter, Query
from sqlmodel import select
from sqlmodel.ext.asyncio.session import AsyncSession
from db import Artifacts
from artifact_db import get_session, init_db, redis_client
from dto.artifact_schema import ArtifactCreate, ArtifactResponse
from artifact_service import create_artifact_service, get_all_artifacts_service, get_all_artifacts_by_tag_service, get_all_artifacts_by_tags_service

router = APIRouter(tags=["artifacts"])

# @router.on_event("startup")
# async def on_startup():
#     await init_db()
#     await redis_client.ping() # type: ignore

# Health Check
@router.get("/health")
async def liveness():
    return {"health": "ok"}

# Get all artifacts
@router.get("/", response_model=List[ArtifactResponse])
async def get_all_artifacts_controller(db: AsyncSession = Depends(get_session)):
    return await get_all_artifacts_service(db)

# Get artifact by tag
@router.get("/by-tag/{tag}", response_model=List[ArtifactResponse])
async def get_all_artifacts_by_tag_controller(tag: str, db: AsyncSession = Depends(get_session)):
    return await get_all_artifacts_by_tag_service(tag,db)

# Get artifact by multiple tags
@router.get("/by-tags", response_model=List[ArtifactResponse])
async def get_all_artifacts_by_tags_controller(tags: List[str] = Query(...), db: AsyncSession = Depends(get_session)):
    return await get_all_artifacts_by_tags_service(tags,db)


# Create an Artifact
@router.post("/create", response_model=ArtifactResponse)
async def create_artifact_controller(artifact_data: ArtifactCreate, db: AsyncSession = Depends(get_session)):
    return await create_artifact_service(artifact_data, db)

# Get Artifact by multiple tags
# Get Artifact by ID
# Update artifacts endpoint
# Delete artifacts endpoint
# Create child artifact
