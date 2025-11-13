from fastapi import FastAPI
from artifact_controller import router as artifact_router

app = FastAPI(title="artifact-service", version="0.1.0")
app.include_router(artifact_router,  prefix="/api/artifacts")