from contextlib import asynccontextmanager

import uvicorn
from fastapi import FastAPI
from fastapi.responses import JSONResponse

from app.config import settings
from app.database import create_db_and_tables

# Import models so SQLModel.metadata is populated before create_all runs
from app.models.artifact import Artifact, ArtifactImage  # noqa: F401
from app.routers import artifacts, images


@asynccontextmanager
async def lifespan(_app: FastAPI):
    await create_db_and_tables()
    yield


app = FastAPI(title="Pintrail Artifact Service", docs_url=None, redoc_url=None, lifespan=lifespan)


@app.get("/health")
async def health():
    return JSONResponse({"status": "ok"})


app.include_router(artifacts.router)
app.include_router(images.router)


def run():
    uvicorn.run(
        "app.main:app",
        host="0.0.0.0",
        port=settings.port,
        reload=False,
    )


if __name__ == "__main__":
    run()
