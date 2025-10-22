from fastapi import FastAPI, Depends
from sqlmodel import select
from sqlmodel.ext.asyncio.session import AsyncSession

from .models import User
from .deps import get_session, init_db, redis_client

app = FastAPI(title="user-service", version="0.1.0")


@app.on_event("startup")
async def on_startup():
    await init_db()
    await redis_client.ping()


@app.get("/healthz/liveness")
async def liveness():
    return {"ok": True}


@app.get("/healthz/readiness")
async def readiness():
    try:
        await redis_client.ping()
        return {"ok": True}
    except Exception:
        return {"ok": False}


@app.get("/api/users")
async def list_users(session: AsyncSession = Depends(get_session)):
    result = await session.execute(select(User))
    return result.scalars().all()


@app.post("/api/users")
async def create_user(user: User, session: AsyncSession = Depends(get_session)):
    session.add(user)
    await session.commit()
    await session.refresh(user)
    return user
