import hmac

from fastapi import Header, HTTPException

from app.config import settings


async def require_api_key(x_api_key: str = Header(default="")) -> None:
    if not hmac.compare_digest(x_api_key, settings.api_key):
        raise HTTPException(status_code=403, detail="Invalid or missing API key")
