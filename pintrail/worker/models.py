"""
Artifact image models — mirrors portal/app/models/artifact.py.
Only the tables the worker reads and writes are defined here.
"""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Optional

from sqlalchemy import Column, DateTime
from sqlmodel import Field, SQLModel


class ImageStatus(str, Enum):
    queued = "queued"
    processing = "processing"
    processed = "processed"
    failed = "failed"


def _now() -> datetime:
    return datetime.now(timezone.utc)


class Artifact(SQLModel, table=True):
    """Stub so SQLAlchemy can resolve the artifact_images.artifact_id foreign key."""
    __tablename__ = "artifacts"

    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    name: str = Field(default="")
    desc: str = Field(default="")
    lat: Optional[float] = Field(default=None)
    lng: Optional[float] = Field(default=None)
    parent_id: Optional[uuid.UUID] = Field(
        default=None, foreign_key="artifacts.id", ondelete="CASCADE"
    )
    created_at: datetime = Field(default_factory=_now, sa_column=Column(DateTime(timezone=True)))
    updated_at: datetime = Field(default_factory=_now, sa_column=Column(DateTime(timezone=True)))


class ArtifactImage(SQLModel, table=True):
    __tablename__ = "artifact_images"

    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    artifact_id: uuid.UUID = Field(foreign_key="artifacts.id", ondelete="CASCADE")
    original_filename: str
    original_mime_type: str
    original_storage_path: str
    status: ImageStatus = Field(default=ImageStatus.queued)
    processed_filename: Optional[str] = Field(default=None)
    processed_mime_type: Optional[str] = Field(default=None)
    width: Optional[int] = Field(default=None)
    height: Optional[int] = Field(default=None)
    error_message: Optional[str] = Field(default=None)
    created_at: datetime = Field(default_factory=_now, sa_column=Column(DateTime(timezone=True)))
    updated_at: datetime = Field(default_factory=_now, sa_column=Column(DateTime(timezone=True)))
