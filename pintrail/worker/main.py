"""
ARQ background worker for artifact image processing.

Converts uploaded images to WebP and resizes to a max dimension.
Image record reads/writes go through the artifact service HTTP API.

Run with:
    uv run arq main.WorkerSettings
"""

import uuid
from pathlib import Path

import httpx
from arq.connections import RedisSettings
from PIL import Image

from config import settings

MAX_DIMENSION = 2048
OUTPUT_FORMAT = "WEBP"
OUTPUT_MIME = "image/webp"


async def startup(ctx) -> None:
    ctx["http"] = httpx.AsyncClient(
        base_url=settings.artifact_service_url,
        headers={"X-API-Key": settings.artifact_api_key},
        timeout=60.0,
    )


async def shutdown(ctx) -> None:
    await ctx["http"].aclose()


async def process_image(ctx, image_id: str) -> None:
    """Process a single artifact image: convert to WebP and resize if needed."""
    http: httpx.AsyncClient = ctx["http"]

    # Fetch image record from artifact service
    resp = await http.get(f"/images/{image_id}")
    if resp.status_code == 404:
        print(f"[worker] Image {image_id} not found, skipping.")
        return
    resp.raise_for_status()
    image_data = resp.json()

    original_storage_path = image_data["original_storage_path"]

    # Mark as processing
    await http.patch(f"/images/{image_id}", json={"status": "processing"})

    try:
        src_path = Path(original_storage_path)
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

        await http.patch(
            f"/images/{image_id}",
            json={
                "status": "processed",
                "processed_filename": out_filename,
                "processed_mime_type": OUTPUT_MIME,
                "width": width,
                "height": height,
                "error_message": None,
            },
        )
        print(f"[worker] Processed {image_id} → {out_filename} ({width}×{height})")

    except Exception as e:
        await http.patch(
            f"/images/{image_id}",
            json={"status": "failed", "error_message": str(e)},
        )
        print(f"[worker] Failed to process {image_id}: {e}")


class WorkerSettings:
    on_startup = startup
    on_shutdown = shutdown
    functions = [process_image]
    redis_settings = RedisSettings(
        host=settings.redis_host,
        port=settings.redis_port,
    )
    queue_name = settings.image_queue_name
    max_jobs = 4
