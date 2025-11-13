from datetime import datetime
from typing import Optional, List
from pydantic import BaseModel, ConfigDict
from uuid import UUID

class ArtifactCreate(BaseModel):
    name: str
    description: Optional[str] = None
    lat: float
    lon: float
    alt: Optional[float] = None
    tags: Optional[List[str]] = None
    owner_id: int

class ArtifactResponse(BaseModel):
    id: UUID
    name: str
    description: Optional[str] = None
    lat: float
    lon: float
    alt: Optional[float] = None
    tags: Optional[List[str]] = None

    owner_id: int 
    parent_id: Optional[UUID] = None
    # children_ids: Optional[List[UUID]] = None

    created_at: datetime
    updated_at: Optional[datetime]

    model_config = ConfigDict(from_attributes=True)


