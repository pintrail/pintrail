# Worker — Architecture & Developer Guide

The worker is the background image-processing service for Pintrail. It runs independently of the portal, pulls jobs from a Redis queue, converts uploaded images to WebP, and writes the results back through the artifact service HTTP API.

---

## Table of Contents

1. [Tech Stack](#tech-stack)
2. [Repository Layout](#repository-layout)
3. [Running Locally](#running-locally)
4. [Environment Variables](#environment-variables)
5. [How ARQ Works](#how-arq-works)
6. [WorkerSettings](#workersettings)
7. [The `process_image` Job](#the-process_image-job)
8. [Artifact Service Communication](#artifact-service-communication)
9. [Image Processing Details](#image-processing-details)
10. [Storage Layout](#storage-layout)
11. [Error Handling & Retries](#error-handling--retries)
12. [Relationship to Other Services](#relationship-to-other-services)
13. [Docker](#docker)

---

## Tech Stack

| Concern | Library |
|---|---|
| Job queue | ARQ (async Redis queue) |
| HTTP client | httpx (async) |
| Image processing | Pillow |
| Dependency management | uv |

The worker has no web framework and no direct database access. Its dependency set is deliberately minimal: no FastAPI, no Jinja2, no Uvicorn, no SQLModel, no asyncpg.

---

## Repository Layout

```
worker/
├── main.py          # WorkerSettings, startup/shutdown hooks, process_image job
├── config.py        # Settings loaded from environment variables
├── Dockerfile
├── pyproject.toml
└── uv.lock
```

The worker is intentionally flat — all logic lives in two files.

---

## Running Locally

Prerequisites: Python 3.12+, `uv`, a running Redis instance, and the artifact service running (so the HTTP API is available).

```bash
cd worker
uv sync                   # create .venv and install dependencies
uv run arq main.WorkerSettings
```

ARQ will connect to Redis, subscribe to the configured queue, and log each job as it is picked up. It runs until killed with Ctrl+C.

---

## Environment Variables

All configuration is read by `config.py` at import time via a `Settings` class.

| Variable | Default | Purpose |
|---|---|---|
| `REDIS_HOST` | `localhost` | Redis hostname |
| `REDIS_PORT` | `6379` | Redis port |
| `IMAGE_QUEUE_NAME` | `artifact-image-processing` | ARQ queue name — must match `IMAGE_QUEUE_NAME` in the artifact service |
| `IMAGE_STORAGE_ROOT` | `./data/images` | Root directory for image files — must resolve to the same physical path as the artifact service storage root |
| `ARTIFACT_SERVICE_URL` | `http://localhost:8001` | Base URL of the artifact service HTTP API |
| `ARTIFACT_API_KEY` | `dev-api-key` | Shared secret sent as `X-API-Key` header on every artifact service request |

`IMAGE_QUEUE_NAME` and `IMAGE_STORAGE_ROOT` must be identical between the worker and artifact service. In Docker Compose both services mount the same named volume (`artifact-images`) at `/data/images`.

---

## How ARQ Works

ARQ is a Redis-backed async job queue for Python. The artifact service enqueues jobs by calling `redis.enqueue_job("process_image", str(image_id))`. ARQ serialises the arguments to Redis. The worker process runs a loop that:

1. Blocks on a Redis list (the queue) waiting for new job messages.
2. Deserialises the job arguments.
3. Calls the registered Python function with those arguments.
4. Records the result (or exception) back to Redis.

---

## WorkerSettings

`WorkerSettings` is a plain class that ARQ reads by convention:

```python
class WorkerSettings:
    on_startup = startup      # creates shared httpx.AsyncClient
    on_shutdown = shutdown    # closes the client
    functions = [process_image]
    redis_settings = RedisSettings(host=..., port=...)
    queue_name = settings.image_queue_name
    max_jobs = 4
```

**`max_jobs = 4`** means up to 4 `process_image` calls can run concurrently within a single worker process. Since image processing is CPU-bound (Pillow), raising this above the CPU core count will not improve throughput. To scale beyond 4 parallel jobs, run multiple worker containers.

### Startup and shutdown hooks

```python
async def startup(ctx):
    ctx["http"] = httpx.AsyncClient(
        base_url=settings.artifact_service_url,
        headers={"X-API-Key": settings.artifact_api_key},
        timeout=60.0,
    )

async def shutdown(ctx):
    await ctx["http"].aclose()
```

A single `AsyncClient` is created when the worker process starts and closed on exit. All jobs share this client.

---

## The `process_image` Job

This is the only job function. It is called by ARQ with two arguments: `ctx` (the ARQ job context dict, which contains `ctx["http"]`) and `image_id` (a string UUID).

### Full execution flow

```
ARQ calls process_image(ctx, image_id)
    │
    ├─ GET /images/{image_id}  → artifact service
    │   └─ If 404 → log and return (job succeeds, no retry)
    │
    ├─ PATCH /images/{image_id} {"status": "processing"}
    │
    ├─ Open original file from original_storage_path
    │   └─ If file missing → raise FileNotFoundError → go to failure path
    │
    ├─ Convert colour mode to RGB or RGBA
    ├─ Resize to MAX_DIMENSION (2048px) if either dimension exceeds it
    ├─ Save as WebP (quality=85, method=6) alongside original
    │
    ├─ PATCH /images/{image_id} {"status": "processed",
    │          "processed_filename": "...", "processed_mime_type": "image/webp",
    │          "width": N, "height": N, "error_message": null}
    └─ Done

    On any exception during processing:
    └─ PATCH /images/{image_id} {"status": "failed", "error_message": "..."}
```

### Status transitions

```
queued  ──► processing  ──► processed
                        └──► failed
```

The artifact service sets `queued` when it creates the `ArtifactImage` row. The worker transitions through `processing` and then to `processed` or `failed`. The portal's image gallery polls `GET /artifacts/{id}/images` every 2 seconds while any image is in `queued` or `processing` state, stopping automatically once all are resolved.

---

## Artifact Service Communication

The worker communicates with the artifact service via two endpoints:

### `GET /images/{image_id}`

Fetches the image record. The worker needs `original_storage_path` to locate the source file on the shared volume.

```python
resp = await http.get(f"/images/{image_id}")
image_data = resp.json()
path = image_data["original_storage_path"]
```

### `PATCH /images/{image_id}`

Updates the image status and metadata. Called twice per job (once for `processing`, once for `processed` or `failed`). Both calls are idempotent.

```python
# Mark as processing
await http.patch(f"/images/{image_id}", json={"status": "processing"})

# Mark as processed
await http.patch(f"/images/{image_id}", json={
    "status": "processed",
    "processed_filename": out_filename,
    "processed_mime_type": "image/webp",
    "width": width,
    "height": height,
    "error_message": None,
})

# Mark as failed
await http.patch(f"/images/{image_id}", json={
    "status": "failed",
    "error_message": str(e),
})
```

All requests include `X-API-Key` automatically (set as a default header on the `AsyncClient`).

---

## Image Processing Details

Processing is performed by Pillow inside a `with Image.open(src_path) as img:` block.

### Colour mode normalisation

```python
img = img.convert("RGBA" if img.mode in ("RGBA", "LA", "P") else "RGB")
```

Images with transparency (`RGBA`, `LA`) or palette mode (`P`) are kept as RGBA. All other modes are converted to RGB.

### Resize

```python
if max(img.width, img.height) > MAX_DIMENSION:
    img.thumbnail((MAX_DIMENSION, MAX_DIMENSION), Image.LANCZOS)
```

`thumbnail()` scales the image down so that neither dimension exceeds `MAX_DIMENSION` (2048px), preserving the aspect ratio. Only ever shrinks.

### Output

```python
img.save(out_path, format="WEBP", quality=85, method=6)
```

- **Format**: WebP — good compression at high quality, widely supported in modern browsers.
- **Quality 85**: good visual fidelity at roughly half the file size of JPEG at equivalent quality.
- **Method 6**: the slowest (highest quality) WebP encoding method. Appropriate for a background job.

The output file is written to the same directory as the original: `IMAGE_STORAGE_ROOT/{artifact_id}/processed_{uuid}.webp`.

---

## Storage Layout

The worker reads original files and writes processed files under `IMAGE_STORAGE_ROOT`. In Docker Compose this is the shared `artifact-images` volume mounted at `/data/images` in the worker and artifact service containers.

```
IMAGE_STORAGE_ROOT/
└── {artifact_id}/                      # one directory per artifact (UUID)
    ├── {uuid}_{original_filename}      # written by the artifact service on upload
    └── processed_{uuid}.webp           # written by the worker after processing
```

---

## Error Handling & Retries

### Job-level errors

If the initial `GET /images/{id}` call returns 404, the worker logs and returns without error (job completes successfully, no retry needed).

If the artifact service is unreachable during the initial fetch, the httpx call raises an exception that propagates to ARQ, which marks the job as failed in Redis. The image record remains in `queued` state (the PATCH to `processing` was never called).

If any exception occurs during file I/O or Pillow processing, the worker calls `PATCH /images/{id}` with `status=failed` before returning. From ARQ's perspective the job completed successfully — no automatic retry.

### Stuck jobs

If the worker process is killed between the `PATCH processing` call and the `PATCH processed/failed` call, the row stays in `processing` indefinitely. To recover:

```sql
UPDATE artifact_images SET status = 'queued' WHERE status = 'processing';
```

---

## Relationship to Other Services

| Shared resource | How |
|---|---|
| Redis queue | Artifact service enqueues `process_image` jobs; worker consumes them |
| Artifact service HTTP API | Worker reads image records and writes status updates |
| Image storage volume | Artifact service writes originals; worker reads originals and writes WebP outputs |

The worker has no direct database connection. All data operations go through the artifact service REST API.

---

## Docker

```dockerfile
FROM python:3.12-slim

COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /usr/local/bin/

WORKDIR /srv/wk2

COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-dev --no-install-project

COPY config.py main.py ./

RUN mkdir -p ./data/images

CMD ["uv", "run", "--no-dev", "arq", "main.WorkerSettings"]
```

There is no `EXPOSE` directive — the worker has no listening port. It only makes outbound connections (to Redis and the artifact service).
