# Worker — Architecture & Developer Guide

The worker is the background image-processing service for Pintrail. It runs independently of the portal, pulls jobs from a Redis queue, converts uploaded images to WebP, and writes the results back to the shared PostgreSQL database and shared image storage volume.

---

## Table of Contents

1. [Tech Stack](#tech-stack)
2. [Repository Layout](#repository-layout)
3. [Running Locally](#running-locally)
4. [Environment Variables](#environment-variables)
5. [How ARQ Works](#how-arq-works)
6. [WorkerSettings](#workersettings)
7. [The `process_image` Job](#the-process_image-job)
8. [Database Access](#database-access)
9. [Data Models](#data-models)
10. [Image Processing Details](#image-processing-details)
11. [Storage Layout](#storage-layout)
12. [Error Handling & Retries](#error-handling--retries)
13. [Relationship to the Portal](#relationship-to-the-portal)
14. [Docker](#docker)

---

## Tech Stack

| Concern | Library |
|---|---|
| Job queue | ARQ (async Redis queue) |
| Database ORM | SQLModel (SQLAlchemy 2 async) |
| Database driver | asyncpg |
| Image processing | Pillow |
| Dependency management | uv |

The worker has no web framework — it is a pure background process. Its dependency set is deliberately minimal compared to the portal: no FastAPI, no Jinja2, no Uvicorn.

---

## Repository Layout

```
worker/
├── main.py          # WorkerSettings, process_image job function, DB session factory
├── models.py        # ArtifactImage model (+ Artifact stub for FK resolution)
├── config.py        # Settings loaded from environment variables
├── Dockerfile
├── pyproject.toml
├── uv.lock
└── .env.example
```

The worker is intentionally flat — there are no subdirectories or packages. All logic lives in three files.

---

## Running Locally

Prerequisites: Python 3.12+, `uv`, a running PostgreSQL instance, a running Redis instance, and the portal running (or at least its DB tables created) so that the `artifact_images` table exists.

```bash
cd worker
cp .env.example .env      # fill in DB and Redis credentials
uv sync                   # create .venv and install dependencies
uv run arq main.WorkerSettings
```

ARQ will connect to Redis, subscribe to the configured queue, and log each job as it is picked up. It runs until killed with Ctrl+C.

The worker does **not** create database tables. Tables are created by the portal on startup. If you run the worker before the portal has ever started, it will fail when it tries to query `artifact_images`.

---

## Environment Variables

All configuration is read by `config.py` at import time via a `Settings` class. The `settings` singleton is imported by `main.py`.

| Variable | Default | Purpose |
|---|---|---|
| `DB_HOST` | `localhost` | PostgreSQL hostname |
| `DB_PORT` | `5432` | PostgreSQL port |
| `DB_USER` | `pintrail` | PostgreSQL user |
| `DB_PASSWORD` | `pintrail` | PostgreSQL password |
| `DB_NAME` | `pintrail` | PostgreSQL database name |
| `REDIS_HOST` | `localhost` | Redis hostname |
| `REDIS_PORT` | `6379` | Redis port |
| `IMAGE_QUEUE_NAME` | `artifact-image-processing` | ARQ queue name — must match `IMAGE_QUEUE_NAME` in the portal |
| `IMAGE_STORAGE_ROOT` | `./data/images` | Root directory for image files — must resolve to the same physical path as the portal's storage root |

`IMAGE_QUEUE_NAME` and `IMAGE_STORAGE_ROOT` must be identical between the worker and portal. In Docker Compose both services mount the same named volume (`artifact-images`) at `/data/images`.

---

## How ARQ Works

ARQ is a Redis-backed async job queue for Python. The portal enqueues jobs by calling `redis.enqueue_job("process_image", str(image_id))`. ARQ serialises the arguments to Redis. The worker process runs a loop that:

1. Blocks on a Redis list (the queue) waiting for new job messages.
2. Deserialises the job arguments.
3. Calls the registered Python function with those arguments.
4. Records the result (or exception) back to Redis.

Jobs are identified by function name. The worker only registers `process_image` — any job arriving with a different function name will be ignored (logged as unknown).

The worker process itself is started by ARQ's CLI runner:

```
arq main.WorkerSettings
```

ARQ reads `WorkerSettings` to discover which functions to register, how to connect to Redis, which queue to watch, and how many jobs to run in parallel.

---

## WorkerSettings

`WorkerSettings` is a plain class (not a dataclass or Pydantic model) that ARQ reads by convention:

```python
class WorkerSettings:
    functions = [process_image]      # job functions this worker handles
    redis_settings = RedisSettings(
        host=settings.redis_host,
        port=settings.redis_port,
    )
    queue_name = settings.image_queue_name   # Redis list key to consume from
    max_jobs = 4                             # maximum concurrent jobs
```

**`max_jobs = 4`** means up to 4 `process_image` calls can run concurrently within a single worker process (as async coroutines on one event loop). Each job opens its own database session. Since image processing is CPU-bound (Pillow), raising this number above the CPU core count will not improve throughput and may cause contention on the asyncpg connection pool. To scale beyond 4 parallel jobs, run multiple worker containers rather than raising `max_jobs`.

---

## The `process_image` Job

This is the only job function. It is called by ARQ with two arguments: `ctx` (an ARQ job context dict, unused here) and `image_id` (a string UUID).

### Full execution flow

```
ARQ calls process_image(ctx, image_id)
    │
    ├─ Open DB session
    ├─ SELECT artifact_images WHERE id = image_id
    │   └─ If not found → log and return (job succeeds, no retry)
    │
    ├─ UPDATE status = 'processing', updated_at = now()
    ├─ COMMIT
    │
    ├─ Open original file from original_storage_path
    │   └─ If file missing → raise FileNotFoundError → go to failure path
    │
    ├─ Convert colour mode to RGB or RGBA
    ├─ Resize to MAX_DIMENSION (2048px) if either dimension exceeds it
    ├─ Save as WebP (quality=85, method=6) alongside original
    │
    ├─ UPDATE status = 'processed'
    │      processed_filename = 'processed_{uuid}.webp'
    │      processed_mime_type = 'image/webp'
    │      width, height = actual output dimensions
    │      error_message = None
    │      updated_at = now()
    └─ COMMIT

    On any exception during processing:
    ├─ UPDATE status = 'failed'
    │      error_message = str(exception)
    │      updated_at = now()
    └─ COMMIT
```

### Status transitions

```
queued  ──► processing  ──► processed
                        └──► failed
```

The portal sets `queued` when it creates the `ArtifactImage` row. The worker transitions through `processing` and then to `processed` or `failed`. The portal's image gallery polls `GET /artifacts/{id}/images` every 2 seconds while any image is in `queued` or `processing` state, stopping automatically once all are resolved.

### Important implementation note: status is committed before processing begins

The status is set to `processing` and committed to the database *before* the image file is opened. This means that if the worker process is killed mid-job, the image will remain in `processing` state indefinitely — it will not automatically revert to `queued`. This is an intentional simplicity trade-off. To recover a stuck image, update its status to `queued` directly in the database.

---

## Database Access

The worker creates its own SQLAlchemy async engine, independent of the portal's. The engine is initialised lazily on first job execution and reused across all subsequent jobs in the same process lifetime:

```python
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
```

Lazy initialisation avoids creating a DB connection at import time, which would fail if the database is not yet ready when the module loads.

Each job call opens a fresh session via `async with session_factory() as db:` and closes it when the `async with` block exits — whether the job succeeds or fails.

`expire_on_commit=False` means the `image` object remains usable after `await db.commit()` without needing a refresh round-trip.

Queries use SQLAlchemy's `execute()` + `.scalars()` pattern explicitly (rather than SQLModel's `exec()`), which had version-dependent behaviour returning raw `Row` objects instead of model instances:

```python
result = await db.execute(select(ArtifactImage).where(...))
image = result.scalars().first()
```

---

## Data Models

`models.py` defines two SQLModel table classes. Both use `sa_column=Column(DateTime(timezone=True))` on all timestamp fields so PostgreSQL stores them as `TIMESTAMPTZ`, which is required when passing timezone-aware Python datetimes via asyncpg.

### `ArtifactImage` (`artifact_images` table)

This is the only table the worker actively reads and writes.

| Column | Type | Worker access |
|---|---|---|
| `id` | UUID PK | read (lookup) |
| `artifact_id` | UUID FK → `artifacts.id` | read only |
| `original_filename` | VARCHAR | read only |
| `original_mime_type` | VARCHAR | read only |
| `original_storage_path` | VARCHAR | read — path to the source file on disk |
| `status` | enum | read + write (`processing` → `processed` / `failed`) |
| `processed_filename` | VARCHAR | write — filename of the WebP output |
| `processed_mime_type` | VARCHAR | write — always `image/webp` |
| `width` | INTEGER | write — output pixel width |
| `height` | INTEGER | write — output pixel height |
| `error_message` | TEXT | write — exception message on failure |
| `updated_at` | TIMESTAMPTZ | write — set to `now()` on each status change |

### `Artifact` (`artifacts` table) — stub only

The worker never reads or writes artifact rows. This class exists solely because SQLAlchemy's ORM metadata must be able to resolve the foreign key `artifact_images.artifact_id → artifacts.id` at session flush time. Without this stub, SQLAlchemy raises `NoReferencedTableError` when attempting to flush an `ArtifactImage` update.

The worker will never call `db.add()` on an `Artifact` instance.

---

## Image Processing Details

Processing is performed by Pillow inside the `with Image.open(src_path) as img:` block.

### Colour mode normalisation

```python
img = img.convert("RGBA" if img.mode in ("RGBA", "LA", "P") else "RGB")
```

Images with transparency (`RGBA`, `LA`) or palette mode (`P`, which may have transparency) are kept as RGBA. All other modes (greyscale, YCbCr, CMYK, etc.) are converted to RGB. This ensures Pillow can save the result as WebP without colour encoding errors.

### Resize

```python
if max(img.width, img.height) > MAX_DIMENSION:
    img.thumbnail((MAX_DIMENSION, MAX_DIMENSION), Image.LANCZOS)
```

`thumbnail()` scales the image down so that neither dimension exceeds `MAX_DIMENSION` (2048px), preserving the aspect ratio. It only ever shrinks — images smaller than 2048px on both axes are left at their original size. `Image.LANCZOS` is a high-quality downsampling filter.

### Output

```python
img.save(out_path, format="WEBP", quality=85, method=6)
```

- **Format**: WebP — good compression at high quality, widely supported in modern browsers.
- **Quality 85**: good visual fidelity at roughly half the file size of JPEG at equivalent quality.
- **Method 6**: the slowest (highest quality) WebP encoding method. This is appropriate for a background job where encoding time is not user-facing.

The output file is written to the same directory as the original: `IMAGE_STORAGE_ROOT/{artifact_id}/processed_{uuid}.webp`. The original file is never modified or deleted.

---

## Storage Layout

Both the portal and worker read and write image files under `IMAGE_STORAGE_ROOT`. In Docker Compose this is the shared `artifact-images` volume mounted at `/data/images` in both containers.

```
IMAGE_STORAGE_ROOT/
└── {artifact_id}/                      # one directory per artifact (UUID)
    ├── {uuid}_{original_filename}      # written by the portal on upload
    └── processed_{uuid}.webp           # written by the worker after processing
```

The worker derives the output path from the input path: it writes the processed file to `src_path.parent / out_filename`, putting it in the same artifact directory as the original.

The portal serves both originals and processed files via `GET /media/{artifact_id}/{filename}`. In practice the gallery only displays processed files — originals are kept as a source of truth and are not exposed in the UI.

---

## Error Handling & Retries

### Job-level errors

If `process_image` raises an unhandled exception outside the `try/except` block (e.g. the DB is down when the initial lookup runs), ARQ will catch it, mark the job as failed in Redis, and log the traceback. The `ArtifactImage` row will remain in `processing` status if the failure happened after the first commit.

If the exception is caught by the inner `try/except` (i.e. during file I/O or Pillow processing), the worker commits `status = failed` with the error message before returning normally. From ARQ's perspective the job completed successfully (no exception propagated), so ARQ will **not** retry it.

### ARQ retry policy

ARQ supports configurable retries per function. The worker does not currently configure `keep_result`, `max_tries`, or `timeout` on `process_image`, so ARQ defaults apply (one attempt, no automatic retry). To add retries, the function signature can be decorated or `WorkerSettings` can declare `job_timeout` and `max_tries`.

### Stuck jobs

If the worker process is killed between the first commit (`status = processing`) and the second commit (`status = processed/failed`), the row stays in `processing` indefinitely. The portal's gallery polls indefinitely for `queued` or `processing` images. To recover:

```sql
UPDATE artifact_images SET status = 'queued' WHERE status = 'processing';
```

---

## Relationship to the Portal

The worker and portal share three things:

| Shared resource | How |
|---|---|
| PostgreSQL database | Both connect with the same credentials; portal creates the schema |
| Redis queue | Portal enqueues `process_image` jobs; worker consumes them |
| Image storage volume | Portal writes originals; worker reads originals and writes WebP outputs |

The services are otherwise fully decoupled. The portal does not call the worker directly and has no knowledge of worker internals. The worker does not call the portal and has no HTTP client. Communication is entirely via the database and queue.

**Schema ownership**: the portal owns the database schema. The worker never calls `create_all` or any DDL. If the schema changes, the portal's startup will apply it, and the worker's models must be updated to match.

**Model duplication**: `models.py` duplicates the `ArtifactImage` definition from `portal/app/models/artifact.py`. This is intentional — the worker is a self-contained service with its own dependency set. If columns are added to `ArtifactImage` in the portal, the worker's `models.py` must be updated in the same change. The `Artifact` stub only needs updating if the `artifacts` table's primary key or name changes.

---

## Docker

The `Dockerfile` follows the same layer-caching pattern as the portal:

```dockerfile
FROM python:3.12-slim

COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /usr/local/bin/

WORKDIR /srv/wk2

# Dependencies layer — only rebuilds when pyproject.toml or uv.lock changes
COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-dev --no-install-project

# Source layer — rebuilds on any code change
COPY config.py models.py main.py ./

RUN mkdir -p ./data/images

CMD ["uv", "run", "--no-dev", "arq", "main.WorkerSettings"]
```

There is no `EXPOSE` directive — the worker has no listening port. It only makes outbound connections (to PostgreSQL and Redis).

The `./data/images` directory created by `RUN mkdir -p` is a placeholder. In compose the volume mount at `/data/images` shadows it, so the actual storage goes to the named volume shared with the portal.
