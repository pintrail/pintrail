import uuid
from datetime import datetime
from typing import Optional

from pydantic import BaseModel

from app.models.artifact import ImageStatus


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

    model_config = {"from_attributes": True}


class ImagePatch(BaseModel):
    status: Optional[ImageStatus] = None
    processed_filename: Optional[str] = None
    processed_mime_type: Optional[str] = None
    width: Optional[int] = None
    height: Optional[int] = None
    error_message: Optional[str] = None
