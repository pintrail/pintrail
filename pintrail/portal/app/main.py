from contextlib import asynccontextmanager

import httpx
import uvicorn
from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from app.config import settings
from app.database import create_db_and_tables, get_session
from app.routers import artifacts, auth, frontend, media
from app.services.auth import create_admin_if_needed


@asynccontextmanager
async def lifespan(_app: FastAPI):
    await create_db_and_tables()
    async for db in get_session():
        await create_admin_if_needed(db)
        break
    async with httpx.AsyncClient(
        base_url=settings.artifact_service_url,
        headers={"X-API-Key": settings.artifact_api_key},
        timeout=30.0,
    ) as client:
        _app.state.artifact_client = client
        yield


app = FastAPI(title="Pintrail pt2", docs_url=None, redoc_url=None, lifespan=lifespan)

app.mount("/static", StaticFiles(directory="app/static"), name="static")

app.include_router(frontend.router)
app.include_router(auth.router)
app.include_router(artifacts.router)
app.include_router(media.router)


def run():
    uvicorn.run(
        "app.main:app",
        host="0.0.0.0",
        port=settings.port,
        reload=not settings.is_production,
    )


if __name__ == "__main__":
    run()
