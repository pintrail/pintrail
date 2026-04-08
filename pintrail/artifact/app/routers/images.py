import uuid
from datetime import datetime, timezone

from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy import select
from sqlmodel.ext.asyncio.session import AsyncSession

from app.database import get_session
from app.dependencies import require_api_key
from app.models.artifact import ArtifactImage
from app.schemas.image import ImagePatch, ImageRead

router = APIRouter(prefix="/images", dependencies=[Depends(require_api_key)])


# ── Get Image (flat, worker-facing) ──────────────────────────────────────────

@router.get("/{image_id}", response_model=ImageRead)
async def get_image(
    image_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
):
    result = await db.execute(select(ArtifactImage).where(ArtifactImage.id == image_id))
    image = result.scalars().first()
    if not image:
        raise HTTPException(status_code=404, detail="Image not found")
    return image


# ── Patch Image (flat, worker-facing) ────────────────────────────────────────

@router.patch("/{image_id}", response_model=ImageRead)
async def patch_image(
    image_id: uuid.UUID,
    body: ImagePatch,
    db: AsyncSession = Depends(get_session),
):
    result = await db.execute(select(ArtifactImage).where(ArtifactImage.id == image_id))
    image = result.scalars().first()
    if not image:
        raise HTTPException(status_code=404, detail="Image not found")

    data = body.model_dump(exclude_unset=True)
    for key, val in data.items():
        setattr(image, key, val)
    image.updated_at = datetime.now(timezone.utc)
    db.add(image)
    await db.commit()
    await db.refresh(image)
    return image
