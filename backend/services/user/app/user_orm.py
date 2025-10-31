from datetime import datetime, timezone
from typing import Optional
from sqlmodel import SQLModel, Field
from uuid import uuid4


class User(SQLModel, table=True):
    __tablename__ = "users"

    id: Optional[int] = Field(default=None, primary_key=True)  # internal PK
    public_id: str = Field(
        default_factory=lambda: str(uuid4()), index=True, unique=True
    )
    username: str = Field(index=True, unique=True)
    email: str = Field(index=True, unique=True)

    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(None, nullable=True)


class Artifacts(SQLModel, table=True):
    __tablename__ = "artifacts"

    id: Optional[int] = Field(default=None, primary_key=True)
    name: str = Field()
    description: Optional[str] = None

    lat: float
    lon: float
    alt: Optional[float] = None

    owner_id: int = Field(foreign_key="users.id")
    parent_id: Optional[int] = Field(default=None, foreign_key="artifacts.id")

    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(None, nullable=True)
