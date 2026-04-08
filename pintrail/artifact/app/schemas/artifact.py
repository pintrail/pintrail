import uuid
from datetime import datetime
from typing import Optional

from pydantic import BaseModel

from app.schemas.image import ImageRead


class ArtifactRead(BaseModel):
    id: uuid.UUID
    name: str
    desc: str
    lat: Optional[float]
    lng: Optional[float]
    parent_id: Optional[uuid.UUID]
    created_at: datetime
    updated_at: datetime

    model_config = {"from_attributes": True}


class ArtifactCreate(BaseModel):
    parent_id: Optional[uuid.UUID] = None


class ArtifactUpdate(BaseModel):
    name: Optional[str] = None
    desc: Optional[str] = None
    lat: Optional[float] = None
    lng: Optional[float] = None


class ArtifactDetailResponse(BaseModel):
    artifact: ArtifactRead
    children: list[ArtifactRead]
    images: list[ImageRead]
