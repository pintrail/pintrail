"""
ARQ background worker for artifact image processing.

Converts uploaded images to WebP and resizes to a max dimension.

Run with:
    uv run arq main.WorkerSettings
"""

import uuid
from datetime import datetime, timezone
from pathlib import Path

from arq.connections import RedisSettings
from PIL import Image
from sqlalchemy import select
from sqlalchemy.ext.asyncio import async_sessionmaker, create_async_engine
from sqlmodel.ext.asyncio.session import AsyncSession

from config import settings
from models import ArtifactImage, ImageStatus

MAX_DIMENSION = 2048
OUTPUT_FORMAT = "WEBP"
OUTPUT_MIME = "image/webp"

_engine = None
_session_factory = None


def _get_session_factory():
    global _engine, _session_factory
    if _session_factory is None:
        _engine = create_async_engine(settings.database_url, echo=False)
        _session_factory = async_sessionmaker(
            _engine, class_=AsyncSession, expire_on_commit=False
        )
    return _session_factory


async def process_image(ctx, image_id: str) -> None:
    """Process a single artifact image: convert to WebP and resize if needed."""
    session_factory = _get_session_factory()

    async with session_factory() as db:
        result = await db.execute(
            select(ArtifactImage).where(ArtifactImage.id == uuid.UUID(image_id))
        )
        image = result.scalars().first()
        if not image:
            print(f"[worker] Image {image_id} not found, skipping.")
            return

        image.status = ImageStatus.processing
        image.updated_at = datetime.now(timezone.utc)
        db.add(image)
        await db.commit()

        try:
            src_path = Path(image.original_storage_path)
            if not src_path.exists():
                raise FileNotFoundError(f"Source file not found: {src_path}")

            with Image.open(src_path) as img:
                img = img.convert("RGBA" if img.mode in ("RGBA", "LA", "P") else "RGB")
                if max(img.width, img.height) > MAX_DIMENSION:
                    img.thumbnail((MAX_DIMENSION, MAX_DIMENSION), Image.LANCZOS)
                width, height = img.size

                out_filename = f"processed_{uuid.uuid4()}.webp"
                out_path = src_path.parent / out_filename
                img.save(out_path, format=OUTPUT_FORMAT, quality=85, method=6)

            image.status = ImageStatus.processed
            image.processed_filename = out_filename
            image.processed_mime_type = OUTPUT_MIME
            image.width = width
            image.height = height
            image.error_message = None
            image.updated_at = datetime.now(timezone.utc)
            db.add(image)
            await db.commit()
            print(f"[worker] Processed {image_id} → {out_filename} ({width}×{height})")

        except Exception as e:
            image.status = ImageStatus.failed
            image.error_message = str(e)
            image.updated_at = datetime.now(timezone.utc)
            db.add(image)
            await db.commit()
            print(f"[worker] Failed to process {image_id}: {e}")


class WorkerSettings:
    functions = [process_image]
    redis_settings = RedisSettings(
        host=settings.redis_host,
        port=settings.redis_port,
    )
    queue_name = settings.image_queue_name
    max_jobs = 4
