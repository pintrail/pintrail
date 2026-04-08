import uuid
from typing import Optional

import httpx
from fastapi import APIRouter, Depends, File, Form, HTTPException, Request, UploadFile
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import require_editor, require_viewer
from app.models.user import User
from app.schemas.artifact import ArtifactRead, ImageRead

router = APIRouter(prefix="/artifacts")
templates = Jinja2Templates(directory="app/templates")


def _client(request: Request) -> httpx.AsyncClient:
    return request.app.state.artifact_client


def _build_tree(artifacts: list[ArtifactRead]) -> list[dict]:
    """Build a nested tree from a flat artifact list."""
    by_id = {a.id: {"artifact": a, "children": []} for a in artifacts}
    roots = []
    for a in artifacts:
        if a.parent_id and a.parent_id in by_id:
            by_id[a.parent_id]["children"].append(by_id[a.id])
        else:
            roots.append(by_id[a.id])
    return roots


def _raise_for_status(resp: httpx.Response) -> None:
    try:
        resp.raise_for_status()
    except httpx.HTTPStatusError as exc:
        raise HTTPException(status_code=exc.response.status_code) from exc


async def _get_all_artifacts(client: httpx.AsyncClient) -> list[ArtifactRead]:
    resp = await client.get("/artifacts")
    _raise_for_status(resp)
    return [ArtifactRead.model_validate(a) for a in resp.json()]


# ── Artifact Tree (sidebar) ───────────────────────────────────────────────────

@router.get("/tree", response_class=HTMLResponse)
async def artifact_tree(
    request: Request,
    selected_id: Optional[uuid.UUID] = None,
    _: User = Depends(require_viewer),
):
    artifacts = await _get_all_artifacts(_client(request))
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
    current_user: User = Depends(require_editor),
):
    client = _client(request)

    resp = await client.post("/artifacts", json={"parent_id": str(parent_id) if parent_id else None})
    _raise_for_status(resp)
    artifact = ArtifactRead.model_validate(resp.json())

    artifacts = await _get_all_artifacts(client)
    tree = _build_tree(artifacts)

    detail_html = templates.get_template("partials/artifact_detail.html").render(
        request=request,
        artifact=artifact,
        artifact_id=artifact.id,
        children=[],
        images=[],
        user=current_user,
    )
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
    current_user: User = Depends(require_viewer),
):
    resp = await _client(request).get(f"/artifacts/{artifact_id}")
    _raise_for_status(resp)
    data = resp.json()

    artifact = ArtifactRead.model_validate(data["artifact"])
    children = [ArtifactRead.model_validate(c) for c in data["children"]]
    images = [ImageRead.model_validate(i) for i in data["images"]]

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
    _: User = Depends(require_editor),
):
    json_body: dict = {"lat": lat, "lng": lng}
    if name is not None:
        json_body["name"] = name
    if desc is not None:
        json_body["desc"] = desc

    resp = await _client(request).patch(f"/artifacts/{artifact_id}", json=json_body)
    _raise_for_status(resp)

    return HTMLResponse('<span class="status-saved">Saved</span>')


# ── Delete Artifact ───────────────────────────────────────────────────────────

@router.delete("/{artifact_id}", response_class=HTMLResponse)
async def delete_artifact(
    request: Request,
    artifact_id: uuid.UUID,
    current_user: User = Depends(require_editor),
):
    client = _client(request)

    resp = await client.delete(f"/artifacts/{artifact_id}")
    _raise_for_status(resp)

    artifacts = await _get_all_artifacts(client)
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
    current_user: User = Depends(require_editor),
):
    files_payload = []
    for upload in files:
        if upload.content_type and upload.content_type.startswith("image/"):
            content = await upload.read()
            files_payload.append(("files", (upload.filename, content, upload.content_type)))

    if files_payload:
        resp = await _client(request).post(
            f"/artifacts/{artifact_id}/images", files=files_payload
        )
        _raise_for_status(resp)
        images = [ImageRead.model_validate(i) for i in resp.json()]
    else:
        images_resp = await _client(request).get(f"/artifacts/{artifact_id}/images")
        _raise_for_status(images_resp)
        images = [ImageRead.model_validate(i) for i in images_resp.json()]

    return templates.TemplateResponse(
        "partials/image_gallery.html",
        {"request": request, "artifact_id": artifact_id, "images": images, "user": current_user},
    )


# ── Image Gallery (polling) ───────────────────────────────────────────────────

@router.get("/{artifact_id}/images", response_class=HTMLResponse)
async def image_gallery(
    request: Request,
    artifact_id: uuid.UUID,
    current_user: User = Depends(require_viewer),
):
    resp = await _client(request).get(f"/artifacts/{artifact_id}/images")
    _raise_for_status(resp)
    images = [ImageRead.model_validate(i) for i in resp.json()]

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
    current_user: User = Depends(require_editor),
):
    resp = await _client(request).delete(f"/artifacts/{artifact_id}/images/{image_id}")
    _raise_for_status(resp)
    images = [ImageRead.model_validate(i) for i in resp.json()]

    return templates.TemplateResponse(
        "partials/image_gallery.html",
        {"request": request, "artifact_id": artifact_id, "images": images, "user": current_user},
    )
