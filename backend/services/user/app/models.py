from datetime import datetime
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
    created_at: datetime = Field(default_factory=datetime.utcnow)
