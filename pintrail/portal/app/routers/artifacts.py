import os
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, Depends, File, Form, HTTPException, Request, UploadFile
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates
from sqlalchemy import select
from sqlmodel.ext.asyncio.session import AsyncSession

from app.config import settings
from app.database import get_session
from app.dependencies import require_editor, require_viewer
from app.models.artifact import Artifact, ArtifactImage, ImageStatus
from app.models.user import User

router = APIRouter(prefix="/artifacts")
templates = Jinja2Templates(directory="app/templates")


def _storage_path(artifact_id: uuid.UUID) -> Path:
    return Path(settings.image_storage_root) / str(artifact_id)


async def _get_artifact_or_404(db: AsyncSession, artifact_id: uuid.UUID) -> Artifact:
    result = await db.execute(select(Artifact).where(Artifact.id == artifact_id))
    artifact = result.scalars().first()
    if not artifact:
        raise HTTPException(status_code=404, detail="Artifact not found")
    return artifact


async def _get_all_artifacts(db: AsyncSession) -> list[Artifact]:
    result = await db.execute(select(Artifact).order_by(Artifact.created_at))
    return list(result.scalars().all())


async def _get_children(db: AsyncSession, artifact_id: uuid.UUID) -> list[Artifact]:
    result = await db.execute(
        select(Artifact)
        .where(Artifact.parent_id == artifact_id)
        .order_by(Artifact.created_at)
    )
    return list(result.scalars().all())


async def _get_images(db: AsyncSession, artifact_id: uuid.UUID) -> list[ArtifactImage]:
    result = await db.execute(
        select(ArtifactImage)
        .where(ArtifactImage.artifact_id == artifact_id)
        .order_by(ArtifactImage.created_at)
    )
    return list(result.scalars().all())


def _build_tree(artifacts: list[Artifact]) -> list[dict]:
    """Build a nested tree from a flat artifact list."""
    by_id = {a.id: {"artifact": a, "children": []} for a in artifacts}
    roots = []
    for a in artifacts:
        if a.parent_id and a.parent_id in by_id:
            by_id[a.parent_id]["children"].append(by_id[a.id])
        else:
            roots.append(by_id[a.id])
    return roots


async def _enqueue_image_processing(image_id: uuid.UUID) -> None:
    """Enqueue image processing job via ARQ."""
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


# ── Artifact Tree (sidebar) ───────────────────────────────────────────────────

@router.get("/tree", response_class=HTMLResponse)
async def artifact_tree(
    request: Request,
    selected_id: Optional[uuid.UUID] = None,
    db: AsyncSession = Depends(get_session),
    _: User = Depends(require_viewer),
):
    artifacts = await _get_all_artifacts(db)
    tree = _build_tree(artifacts)
    return templates.TemplateResponse(
        "partials/artifact_tree.html",
        {"request": request, "tree": tree, "selected_id": selected_id},
    )


# ── Create Artifact ───────────────────────────────────────────────────────────

@router.post("", response_class=HTMLResponse)
async def create_artifact(
    request: Request,
    parent_id: Optional[uuid.UUID] = Form(default=None),
    db: AsyncSession = Depends(get_session),
    current_user: User = Depends(require_editor),
):
    artifact = Artifact(parent_id=parent_id)
    db.add(artifact)
    await db.commit()
    await db.refresh(artifact)

    images: list[ArtifactImage] = []
    children: list[Artifact] = []
    detail_html = templates.get_template("partials/artifact_detail.html").render(
        request=request,
        artifact=artifact,
        artifact_id=artifact.id,
        children=children,
        images=images,
        user=current_user,
    )

    artifacts = await _get_all_artifacts(db)
    tree = _build_tree(artifacts)
    tree_html = templates.get_template("partials/artifact_tree.html").render(
        request=request,
        tree=tree,
        selected_id=artifact.id,
    )

    return HTMLResponse(
        tree_html
        + f'<div id="main-content" hx-swap-oob="innerHTML">{detail_html}</div>'
    )


# ── Artifact Detail ───────────────────────────────────────────────────────────

@router.get("/{artifact_id}", response_class=HTMLResponse)
async def artifact_detail(
    request: Request,
    artifact_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
    current_user: User = Depends(require_viewer),
):
    artifact = await _get_artifact_or_404(db, artifact_id)
    children = await _get_children(db, artifact_id)
    images = await _get_images(db, artifact_id)
    return templates.TemplateResponse(
        "partials/artifact_detail.html",
        {
            "request": request,
            "artifact": artifact,
            "artifact_id": artifact.id,
            "children": children,
            "images": images,
            "user": current_user,
        },
    )


# ── Update Artifact (autosave) ────────────────────────────────────────────────

@router.patch("/{artifact_id}", response_class=HTMLResponse)
async def update_artifact(
    request: Request,
    artifact_id: uuid.UUID,
    name: Optional[str] = Form(default=None),
    desc: Optional[str] = Form(default=None),
    lat: Optional[float] = Form(default=None),
    lng: Optional[float] = Form(default=None),
    db: AsyncSession = Depends(get_session),
    _: User = Depends(require_editor),
):
    artifact = await _get_artifact_or_404(db, artifact_id)
    if name is not None:
        artifact.name = name
    if desc is not None:
        artifact.desc = desc
    artifact.lat = lat
    artifact.lng = lng
    artifact.updated_at = datetime.now(timezone.utc)
    db.add(artifact)
    await db.commit()

    return HTMLResponse('<span class="status-saved">Saved</span>')


# ── Delete Artifact ───────────────────────────────────────────────────────────

@router.delete("/{artifact_id}", response_class=HTMLResponse)
async def delete_artifact(
    request: Request,
    artifact_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
    current_user: User = Depends(require_editor),
):
    artifact = await _get_artifact_or_404(db, artifact_id)
    await db.delete(artifact)
    await db.commit()

    artifacts = await _get_all_artifacts(db)
    tree = _build_tree(artifacts)
    tree_html = templates.get_template("partials/artifact_tree.html").render(
        request=request,
        tree=tree,
        selected_id=None,
    )
    return HTMLResponse(
        tree_html
        + '<div id="main-content" hx-swap-oob="innerHTML">'
        + '<p class="empty-state">Select an artifact from the sidebar.</p>'
        + "</div>"
    )


# ── Image Upload ──────────────────────────────────────────────────────────────

@router.post("/{artifact_id}/images", response_class=HTMLResponse)
async def upload_images(
    request: Request,
    artifact_id: uuid.UUID,
    files: list[UploadFile] = File(...),
    db: AsyncSession = Depends(get_session),
    current_user: User = Depends(require_editor),
):
    await _get_artifact_or_404(db, artifact_id)
    storage_dir = _storage_path(artifact_id)
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

    images = await _get_images(db, artifact_id)
    return templates.TemplateResponse(
        "partials/image_gallery.html",
        {"request": request, "artifact_id": artifact_id, "images": images, "user": current_user},
    )


# ── Image Gallery (polling) ───────────────────────────────────────────────────

@router.get("/{artifact_id}/images", response_class=HTMLResponse)
async def image_gallery(
    request: Request,
    artifact_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
    current_user: User = Depends(require_viewer),
):
    await _get_artifact_or_404(db, artifact_id)
    images = await _get_images(db, artifact_id)
    return templates.TemplateResponse(
        "partials/image_gallery.html",
        {"request": request, "artifact_id": artifact_id, "images": images, "user": current_user},
    )


# ── Delete Image ──────────────────────────────────────────────────────────────

@router.delete("/{artifact_id}/images/{image_id}", response_class=HTMLResponse)
async def delete_image(
    request: Request,
    artifact_id: uuid.UUID,
    image_id: uuid.UUID,
    db: AsyncSession = Depends(get_session),
    current_user: User = Depends(require_editor),
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

    for path_str in [image.original_storage_path, _processed_path(image)]:
        if path_str:
            try:
                Path(path_str).unlink(missing_ok=True)
            except Exception:
                pass

    await db.delete(image)
    await db.commit()

    images = await _get_images(db, artifact_id)
    return templates.TemplateResponse(
        "partials/image_gallery.html",
        {"request": request, "artifact_id": artifact_id, "images": images, "user": current_user},
    )


def _processed_path(image: ArtifactImage) -> Optional[str]:
    if not image.processed_filename:
        return None
    storage_dir = _storage_path(image.artifact_id)
    return str(storage_dir / image.processed_filename)
