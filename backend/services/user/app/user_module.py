from fastapi import FastAPI
from .user_controller import router as user_router

app = FastAPI(title="user-service", version="0.1.0")
app.include_router(user_router, prefix="/api/users")