import uuid
from typing import Optional

from fastapi import APIRouter, Depends, Form, HTTPException, Request, Response, status
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.templating import Jinja2Templates
from sqlmodel.ext.asyncio.session import AsyncSession

from app.database import get_session
from app.dependencies import COOKIE_NAME, get_optional_user, require_admin, require_user
from app.models.user import User, UserRole
from app.services import auth as auth_service

router = APIRouter(prefix="/auth")
templates = Jinja2Templates(directory="app/templates")


@router.post("/login")
async def login(
    request: Request,
    response: Response,
    email: str = Form(...),
    password: str = Form(...),
    db: AsyncSession = Depends(get_session),
):
    token = await auth_service.login(db, email, password)
    if not token:
        return templates.TemplateResponse(
            "login.html",
            {"request": request, "error": "Invalid email or password"},
            status_code=status.HTTP_401_UNAUTHORIZED,
        )

    resp = RedirectResponse(url="/", status_code=status.HTTP_303_SEE_OTHER)
    resp.set_cookie(
        key=COOKIE_NAME,
        value=token,
        httponly=True,
        samesite="lax",
        secure=False,  # set True in production behind HTTPS
        path="/",
    )
    return resp


@router.post("/logout")
async def logout(
    request: Request,
    db: AsyncSession = Depends(get_session),
):
    token = request.cookies.get(COOKIE_NAME)
    if token:
        await auth_service.logout(db, token)
    resp = RedirectResponse(url="/", status_code=status.HTTP_303_SEE_OTHER)
    resp.delete_cookie(COOKIE_NAME)
    return resp


@router.get("/users", response_class=HTMLResponse)
async def list_users(
    request: Request,
    db: AsyncSession = Depends(get_session),
    _: User = Depends(require_admin),
):
    users = await auth_service.list_users(db)
    return templates.TemplateResponse(
        "partials/user_list.html",
        {"request": request, "users": users},
    )


@router.post("/users", response_class=HTMLResponse)
async def create_user(
    request: Request,
    email: str = Form(...),
    password: str = Form(...),
    role: UserRole = Form(UserRole.viewer),
    db: AsyncSession = Depends(get_session),
    current_user: User = Depends(require_admin),
):
    try:
        await auth_service.create_user(db, email, password, role)
    except Exception as e:
        users = await auth_service.list_users(db)
        return templates.TemplateResponse(
            "partials/user_list.html",
            {"request": request, "users": users, "error": str(e)},
        )
    users = await auth_service.list_users(db)
    return templates.TemplateResponse(
        "partials/user_list.html",
        {"request": request, "users": users},
    )


@router.patch("/users/{user_id}", response_class=HTMLResponse)
async def update_user(
    request: Request,
    user_id: uuid.UUID,
    role: Optional[UserRole] = Form(default=None),
    is_active: Optional[bool] = Form(default=None),
    password: Optional[str] = Form(default=None),
    db: AsyncSession = Depends(get_session),
    _: User = Depends(require_admin),
):
    updated = await auth_service.update_user(
        db, user_id, role=role, is_active=is_active, password=password or None
    )
    if not updated:
        raise HTTPException(status_code=404, detail="User not found")
    users = await auth_service.list_users(db)
    return templates.TemplateResponse(
        "partials/user_list.html",
        {"request": request, "users": users},
    )
