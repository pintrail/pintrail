import uuid
from datetime import datetime, timezone

from sqlalchemy import Column, DateTime
from sqlmodel import Field, SQLModel


def _now() -> datetime:
    return datetime.now(timezone.utc)


class NotifySignup(SQLModel, table=True):
    __tablename__ = "notify_signups"

    id: uuid.UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    email: str = Field(unique=True, index=True)
    created_at: datetime = Field(default_factory=_now, sa_column=Column(DateTime(timezone=True)))
