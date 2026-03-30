import re
from pathlib import Path

from fastapi import APIRouter, HTTPException
from fastapi.responses import FileResponse

from app.config import settings

router = APIRouter(prefix="/media")

# Only allow alphanumeric, hyphens, underscores, dots in path segments
_SAFE_SEGMENT = re.compile(r"^[a-zA-Z0-9_\-\.]+$")


def _validate_segment(segment: str) -> None:
    if not _SAFE_SEGMENT.match(segment):
        raise HTTPException(status_code=400, detail="Invalid path")


@router.get("/{folder}/{filename}")
async def serve_media(folder: str, filename: str):
    _validate_segment(folder)
    _validate_segment(filename)

    storage_root = Path(settings.image_storage_root).resolve()
    file_path = (storage_root / folder / filename).resolve()

    # Prevent path traversal
    if not str(file_path).startswith(str(storage_root)):
        raise HTTPException(status_code=403, detail="Forbidden")

    if not file_path.exists() or not file_path.is_file():
        raise HTTPException(status_code=404, detail="File not found")

    return FileResponse(file_path)
