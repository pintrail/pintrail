from db import Artifacts
from sqlmodel.ext.asyncio.session import AsyncSession
from dto.artifact_schema import ArtifactCreate
from sqlmodel import select
from typing import List

async def create_artifact_service(artifact_data: ArtifactCreate, db: AsyncSession):
    new_artifact = Artifacts(
        name = artifact_data.name,
        description = artifact_data.description,
        lat = artifact_data.lat,
        lon = artifact_data.lon,
        alt = artifact_data.alt,
        tags = artifact_data.tags,
        owner_id = artifact_data.owner_id,
        parent_id= artifact_data.parent_id
    )

    db.add(new_artifact)
    await db.commit()
    await db.refresh(new_artifact)
    return new_artifact

async def get_all_artifacts_service(db: AsyncSession):
    return (await db.exec(select(Artifacts))).all()

async def get_artifact_by_id_service(id: str, db: AsyncSession):
    return (await db.exec(select(Artifacts).where(Artifacts.id == id))).all()

async def get_all_artifacts_by_tag_service(tag: str, db: AsyncSession):
    return (await db.exec(select(Artifacts).where(Artifacts.tags.any(tag)))).all()

async def get_all_artifacts_by_tags_service(tags: List[str], db: AsyncSession):
    return (await db.exec(select(Artifacts).where(Artifacts.tags.contains(tags)))).all()
