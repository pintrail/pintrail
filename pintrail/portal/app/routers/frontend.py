from typing import Optional

from fastapi import APIRouter, Depends, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates
from sqlmodel.ext.asyncio.session import AsyncSession

from app.database import get_session
from app.dependencies import get_optional_user, require_admin
from app.models.user import User
from app.services import auth as auth_service

router = APIRouter()
templates = Jinja2Templates(directory="app/templates")


@router.get("/", response_class=HTMLResponse)
async def index(
    request: Request,
    user: Optional[User] = Depends(get_optional_user),
):
    if not user:
        return templates.TemplateResponse("login.html", {"request": request})
    return templates.TemplateResponse("index.html", {"request": request, "user": user})


@router.get("/partials/admin", response_class=HTMLResponse)
async def admin_partial(
    request: Request,
    db: AsyncSession = Depends(get_session),
    current_user: User = Depends(require_admin),
):
    users = await auth_service.list_users(db)
    return templates.TemplateResponse(
        "partials/admin_panel.html",
        {"request": request, "users": users, "current_user": current_user},
    )
