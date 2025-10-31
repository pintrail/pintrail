from datetime import datetime
from typing import Annotated

from pydantic import BaseModel, ConfigDict, EmailStr, StringConstraints, constr

# Input DTO: what clients are allowed to send
Username = Annotated[
    str, StringConstraints(strip_whitespace=True, min_length=3, max_length=50)
]


class UserCreate(BaseModel):
    username: Username
    email: EmailStr


# Output DTO: what we return
class UserRead(BaseModel):
    # Need this to use UserRead.model_validate(user) if
    # user is a User SQLModel. I think this is kind of
    # lame and feels gross.
    model_config = ConfigDict(from_attributes=True)
    id: int
    public_id: str
    username: str
    email: EmailStr
    created_at: datetime
