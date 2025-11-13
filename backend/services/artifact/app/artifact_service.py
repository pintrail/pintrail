from sqlalchemy.orm import Session
from db import Artifacts
from sqlmodel.ext.asyncio.session import AsyncSession
from dto.artifact_schema import ArtifactCreate

async def create_artifact_service(artifact_data: ArtifactCreate, db: AsyncSession):
    new_artifact = Artifacts(
        name = artifact_data.name,
        description = artifact_data.description,
        lat = artifact_data.lat,
        lon = artifact_data.lon,
        alt = artifact_data.alt,
        tags = artifact_data.tags,
        owner_id = artifact_data.owner_id
    )

    db.add(new_artifact)
    await db.commit()
    await db.refresh(new_artifact)
    return new_artifact