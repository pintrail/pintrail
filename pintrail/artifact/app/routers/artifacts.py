import shutil
import math
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

# ── Distance Ranking Formula ────────────────────────────────────────────────────────────

def _distance_meters(lat1: float, lng1: float, lat2: float, lng2: float) -> float:
    """Calculate distance between two latitude/longitude points using Haversine."""
    earth_radius_m = 6371000

    phi1 = math.radians(lat1)
    phi2 = math.radians(lat2)
    delta_phi = math.radians(lat2 - lat1)
    delta_lambda = math.radians(lng2 - lng1)

    a = (
        math.sin(delta_phi / 2) ** 2
        + math.cos(phi1) * math.cos(phi2) * math.sin(delta_lambda / 2) ** 2
    )

    return earth_radius_m * 2 * math.atan2(math.sqrt(a), math.sqrt(1 - a))


@router.get("/nearby", response_model=list[ArtifactRead])
async def nearby_artifacts(
    lat: float,
    lng: float,
    radius_meters: float = 1000,
    distance_weight: float = 0.5,
    popularity_weight: float = 0.3,
    recency_weight: float = 0.2,
    decay_constant: float = 30,
    db: AsyncSession = Depends(get_session),
):
    result = await db.execute(select(Artifact))
    artifacts = list(result.scalars().all())

    max_popularity = max(
        (getattr(artifact, "popularity_count", 0) for artifact in artifacts),
        default=1,
    )

    ranked_artifacts = []

    for artifact in artifacts:
        if artifact.lat is None or artifact.lng is None:
            continue

        distance = _distance_meters(lat, lng, artifact.lat, artifact.lng)

        if distance > radius_meters:
            continue

        distance_score = 1 / (1 + distance / radius_meters)

        popularity_count = getattr(artifact, "popularity_count", 0)
        popularity_score = math.log(1 + popularity_count) / math.log(1 + max_popularity)

        updated_at = artifact.updated_at
        if updated_at.tzinfo is None:
            updated_at = updated_at.replace(tzinfo=timezone.utc)

        days_since_update = (datetime.now(timezone.utc) - updated_at).days
        recency_score = math.exp(-days_since_update / decay_constant)

        final_score = (
            distance_weight * distance_score
            + popularity_weight * popularity_score
            + recency_weight * recency_score
        )

        ranked_artifacts.append((final_score, artifact))

    ranked_artifacts.sort(key=lambda item: item[0], reverse=True)

    return [artifact for _, artifact in ranked_artifacts]

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
