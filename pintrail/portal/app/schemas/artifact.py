"""
Thin Pydantic models for parsing artifact service JSON responses.
These are read-only view models used to pass typed objects to Jinja2 templates.
"""
import uuid
from datetime import datetime
from enum import Enum
from typing import Optional

from pydantic import BaseModel


class ImageStatus(str, Enum):
    queued = "queued"
    processing = "processing"
    processed = "processed"
    failed = "failed"


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


class ImageRead(BaseModel):
    id: uuid.UUID
    artifact_id: uuid.UUID
    original_filename: str
    original_mime_type: str
    original_storage_path: str
    status: ImageStatus
    processed_filename: Optional[str]
    processed_mime_type: Optional[str]
    width: Optional[int]
    height: Optional[int]
    error_message: Optional[str]
    created_at: datetime
    updated_at: datetime
