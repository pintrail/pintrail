import shutil
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, Depends, File, HTTPException, UploadFile
from sqlalchemy import select
from sqlmodel.ext.asyncio.session import AsyncSession

from app.config import settings
from app.database import get_session
from app.dependencies import require_api_key
from app.models.artifact import Artifact, ArtifactImage, ImageStatus
from app.schemas.artifact import ArtifactCreate, ArtifactDetailResponse, ArtifactRead, ArtifactUpdate
from app.schemas.image import ImageRead

router = APIRouter(prefix="/artifacts", dependencies=[Depends(require_api_key)])


def _storage_dir(artifact_id: uuid.UUID) -> Path:
    return Path(settings.image_storage_root) / str(artifact_id)


def _processed_path(image: ArtifactImage) -> Optional[Path]:
    if not image.processed_filename:
        return None
    return _storage_dir(image.artifact_id) / image.processed_filename


async def _get_artifact_or_404(db: AsyncSession, artifact_id: uuid.UUID) -> Artifact:
    result = await db.execute(select(Artifact).where(Artifact.id == artifact_id))
    artifact = result.scalars().first()
    if not artifact:
        raise HTTPException(status_code=404, detail="Artifact not found")
    return artifact


async def _get_images(db: AsyncSession, artifact_id: uuid.UUID) -> list[ArtifactImage]:
    result = await db.execute(
        select(ArtifactImage)
        .where(ArtifactImage.artifact_id == artifact_id)
        .order_by(ArtifactImage.created_at)
    )
    return list(result.scalars().all())


async def _enqueue_image_processing(image_id: uuid.UUID) -> None:
    try:
        from arq.connections import RedisSettings, create_pool
        redis = await create_pool(
            RedisSettings(host=settings.redis_host, port=settings.redis_port),
        )
        await redis.enqueue_job(
            "process_image",
            str(image_id),
            _queue_name=settings.image_queue_name,
        )
        await redis.aclose()
    except Exception as e:
        print(f"[warn] Could not enqueue image job: {e}")


# ── List Artifacts ────────────────────────────────────────────────────────────

@router.get("", response_model=list[ArtifactRead])
async def list_artifacts(db: AsyncSession = Depends(get_session)):
    result = await db.execute(select(Artifact).order_by(Artifact.created_at))
    return list(result.scalars().all())


# ── Create Artifact ───────────────────────────────────────────────────────────

@router.post("", response_model=ArtifactRead, status_code=201)
async def create_artifact(
    body: ArtifactCreate,
    db: AsyncSession = Depends(get_session),
):
    artifact = Artifact(parent_id=body.parent_id)
    db.add(artifact)
    await db.commit()
    await db.refresh(artifact)
    return artifact


# ── Get Artifact Detail ───────────────────────────────────────────────────────

@router.get("/{artifact_id}", response_model=ArtifactDetailResponse)
async def get_artifact(
    artifact_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
):
    artifact = await _get_artifact_or_404(db, artifact_id)

    children_result = await db.execute(
        select(Artifact)
        .where(Artifact.parent_id == artifact_id)
        .order_by(Artifact.created_at)
    )
    children = list(children_result.scalars().all())
    images = await _get_images(db, artifact_id)

    return ArtifactDetailResponse(
        artifact=ArtifactRead.model_validate(artifact),
        children=[ArtifactRead.model_validate(c) for c in children],
        images=[ImageRead.model_validate(i) for i in images],
    )


# ── Update Artifact ───────────────────────────────────────────────────────────

@router.patch("/{artifact_id}", response_model=ArtifactRead)
async def update_artifact(
    artifact_id: uuid.UUID,
    body: ArtifactUpdate,
    db: AsyncSession = Depends(get_session),
):
    artifact = await _get_artifact_or_404(db, artifact_id)
    data = body.model_dump(exclude_unset=True)
    for key, val in data.items():
        setattr(artifact, key, val)
    artifact.updated_at = datetime.now(timezone.utc)
    db.add(artifact)
    await db.commit()
    await db.refresh(artifact)
    return artifact


# ── Delete Artifact ───────────────────────────────────────────────────────────

@router.delete("/{artifact_id}", status_code=204)
async def delete_artifact(
    artifact_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
):
    artifact = await _get_artifact_or_404(db, artifact_id)
    storage_dir = _storage_dir(artifact_id)
    if storage_dir.exists():
        shutil.rmtree(storage_dir, ignore_errors=True)
    await db.delete(artifact)
    await db.commit()


# ── List Images ───────────────────────────────────────────────────────────────

@router.get("/{artifact_id}/images", response_model=list[ImageRead])
async def list_images(
    artifact_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
):
    await _get_artifact_or_404(db, artifact_id)
    return await _get_images(db, artifact_id)


# ── Upload Images ─────────────────────────────────────────────────────────────

@router.post("/{artifact_id}/images", response_model=list[ImageRead])
async def upload_images(
    artifact_id: uuid.UUID,
    files: list[UploadFile] = File(...),
    db: AsyncSession = Depends(get_session),
):
    await _get_artifact_or_404(db, artifact_id)
    storage_dir = _storage_dir(artifact_id)
    storage_dir.mkdir(parents=True, exist_ok=True)

    for upload in files:
        if not upload.content_type or not upload.content_type.startswith("image/"):
            continue

        filename = f"{uuid.uuid4()}_{upload.filename}"
        dest_path = storage_dir / filename
        content = await upload.read()
        dest_path.write_bytes(content)

        image = ArtifactImage(
            artifact_id=artifact_id,
            original_filename=upload.filename or filename,
            original_mime_type=upload.content_type,
            original_storage_path=str(dest_path),
            status=ImageStatus.queued,
        )
        db.add(image)
        await db.commit()
        await db.refresh(image)
        await _enqueue_image_processing(image.id)

    return await _get_images(db, artifact_id)


# ── Delete Image ──────────────────────────────────────────────────────────────

@router.delete("/{artifact_id}/images/{image_id}", response_model=list[ImageRead])
async def delete_image(
    artifact_id: uuid.UUID,
    image_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
):
    result = await db.execute(
        select(ArtifactImage).where(
            ArtifactImage.id == image_id,
            ArtifactImage.artifact_id == artifact_id,
        )
    )
    image = result.scalars().first()
    if not image:
        raise HTTPException(status_code=404, detail="Image not found")

    for path in [Path(image.original_storage_path), _processed_path(image)]:
        if path:
            try:
                path.unlink(missing_ok=True)
            except Exception:
                pass

    await db.delete(image)
    await db.commit()

    return await _get_images(db, artifact_id)
