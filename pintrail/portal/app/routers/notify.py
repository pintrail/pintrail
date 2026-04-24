import re

from fastapi import APIRouter, Depends, status
from fastapi.responses import JSONResponse
from sqlalchemy.exc import IntegrityError
from sqlmodel.ext.asyncio.session import AsyncSession

from app.database import get_session
from app.models.notify import NotifySignup
from app.services.email import send_notify_confirmation

router = APIRouter(prefix="/notify")

_EMAIL_RE = re.compile(r"^[^@\s]+@[^@\s]+\.[^@\s]+$")


@router.post("")
async def notify_signup(
    email: str,
    db: AsyncSession = Depends(get_session),
):
    email = email.strip().lower()
    if not _EMAIL_RE.match(email):
        return JSONResponse(
            {"error": "Invalid email address"},
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
        )

    signup = NotifySignup(email=email)
    db.add(signup)
    try:
        await db.commit()
    except IntegrityError:
        await db.rollback()
        # Already signed up — treat as success so we don't leak existence
        return JSONResponse({"ok": True})

    await send_notify_confirmation(email)
    return JSONResponse({"ok": True}, status_code=status.HTTP_201_CREATED)
