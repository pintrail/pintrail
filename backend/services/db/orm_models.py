from datetime import datetime, timezone
from typing import Optional, List
from sqlmodel import SQLModel, Field, Relationship, Column, String
from uuid import uuid4, UUID
from sqlalchemy.dialects.postgresql import ARRAY

def gen_uuid():
    return uuid4()

class User(SQLModel, table=True):
    __tablename__ = "users"

    id: Optional[int] = Field(default=None, primary_key=True)  # internal PK
    public_id: str = Field(
        default_factory=lambda: str(uuid4()), index=True, unique=True
    )
    username: str = Field(index=True, unique=True)
    email: str = Field(index=True, unique=True)

    artifacts: List["Artifacts"] = Relationship(back_populates="owner")
    
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(None, nullable=True)


class Artifacts(SQLModel, table=True):
    __tablename__ = "artifacts"

    # Generated Id
    id: Optional[UUID] = Field(default_factory=gen_uuid, primary_key=True)

    # Basic Info
    name: str 
    description: Optional[str] = None
    lat: float
    lon: float
    alt: Optional[float] = None
    tags: Optional[List[str]] = Field(default_factory=list, sa_column=Column(ARRAY(String))) # There can probably be some relationship defined between tags for easier filtering

    # Related Ids
    owner_id: Optional[int] = Field(foreign_key="users.id", default=None)
    parent_id: Optional[UUID] = Field(default= None, foreign_key="artifacts.id")
    
    # Relationships 
    owner: Optional["User"] = Relationship(back_populates="artifacts")
    parent: Optional["Artifacts"] = Relationship(back_populates="children", sa_relationship_kwargs={"remote_side": "Artifacts.id"})
    children: List["Artifacts"] = Relationship(back_populates="parent")

    # Time stamps
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: Optional[datetime] = Field(default=None, nullable=True)

