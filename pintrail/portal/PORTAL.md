# Portal — Architecture & Developer Guide

The portal is the web-facing service for Pintrail. It is a Python application built with FastAPI, SQLModel, Jinja2, and HTMX. It serves a server-rendered UI, manages users and artifacts, handles image uploads, and delegates image processing to the worker service via a Redis queue.

---

## Table of Contents

1. [Tech Stack](#tech-stack)
2. [Repository Layout](#repository-layout)
3. [Running Locally](#running-locally)
4. [Environment Variables](#environment-variables)
5. [Application Startup](#application-startup)
6. [Database](#database)
7. [Data Models](#data-models)
8. [Authentication & Sessions](#authentication--sessions)
9. [Authorization & Role System](#authorization--role-system)
10. [Routes Reference](#routes-reference)
11. [Services](#services)
12. [Template Architecture (HTMX)](#template-architecture-htmx)
13. [Image Upload & Processing Pipeline](#image-upload--processing-pipeline)
14. [Static Files & Media Serving](#static-files--media-serving)
15. [Docker](#docker)
16. [External Dependencies](#external-dependencies)

---

## Tech Stack

| Concern | Library |
|---|---|
| Web framework | FastAPI 0.115.x |
| ASGI server | Uvicorn |
| ORM | SQLModel (SQLAlchemy 2 async) |
| Database driver | asyncpg |
| Templates | Jinja2 (via Starlette) |
| Frontend interaction | HTMX 2.0 |
| Job queue client | ARQ (Redis) |
| Dependency management | uv |

FastAPI is pinned to `<0.116.0` because Starlette 1.0.0 (pulled by later FastAPI versions) broke Jinja2 template caching.

---

## Repository Layout

```
portal/
├── app/
│   ├── main.py            # FastAPI app factory, lifespan, router registration
│   ├── config.py          # Settings loaded from environment variables
│   ├── database.py        # SQLAlchemy engine, session factory, table creation
│   ├── dependencies.py    # FastAPI dependency functions for auth/role checks
│   ├── models/
│   │   ├── user.py        # User, UserSession, UserRole models
│   │   └── artifact.py    # Artifact, ArtifactImage, ImageStatus models
│   ├── routers/
│   │   ├── frontend.py    # GET / (index + login), GET /partials/admin
│   │   ├── auth.py        # /auth/* — login, logout, user CRUD
│   │   ├── artifacts.py   # /artifacts/* — artifact and image CRUD
│   │   └── media.py       # /media/{folder}/{filename} — image file serving
│   ├── services/
│   │   └── auth.py        # Password hashing, session management, user operations
│   ├── templates/
│   │   ├── base.html      # HTML shell, loads CSS and HTMX
│   │   ├── login.html     # Standalone login page
│   │   ├── index.html     # Authenticated portal shell
│   │   └── partials/
│   │       ├── artifact_tree.html    # Sidebar tree (HTMX partial)
│   │       ├── artifact_detail.html  # Main content panel (HTMX partial)
│   │       ├── image_gallery.html    # Image grid + upload dropzone (HTMX partial)
│   │       ├── admin_panel.html      # Admin panel wrapper
│   │       └── user_list.html        # User management table (HTMX partial)
│   └── static/
│       └── styles.css     # All application CSS (CSS variables, dark theme)
├── Dockerfile
├── pyproject.toml
└── uv.lock
```

---

## Running Locally

Prerequisites: Python 3.12+, `uv`, a running PostgreSQL instance, a running Redis instance.

```bash
cd portal
cp .env.example .env        # fill in DB and Redis credentials
uv sync                     # create .venv and install dependencies
uv run uvicorn app.main:app --reload --port 8000
```

The worker (image processing) is a separate service in `../worker/`. The portal enqueues jobs to Redis; if no worker is running, uploads will remain in `queued` status indefinitely. This does not break the portal itself.

---

## Environment Variables

All configuration is read by `app/config.py` at import time. The `settings` singleton is used everywhere.

| Variable | Default | Purpose |
|---|---|---|
| `DB_HOST` | `localhost` | PostgreSQL hostname |
| `DB_PORT` | `5432` | PostgreSQL port |
| `DB_USER` | `pintrail` | PostgreSQL user |
| `DB_PASSWORD` | `pintrail` | PostgreSQL password |
| `DB_NAME` | `pintrail` | PostgreSQL database name |
| `PORT` | `8000` | Uvicorn listen port |
| `ENV` | `development` | Set to `production` to disable auto-reload |
| `AUTH_ADMIN_EMAIL` | _(empty)_ | Seed admin email on first startup |
| `AUTH_ADMIN_PASSWORD` | _(empty)_ | Seed admin password on first startup |
| `AUTH_SESSION_TTL_HOURS` | `24` | How long session tokens remain valid |
| `REDIS_HOST` | `localhost` | Redis hostname |
| `REDIS_PORT` | `6379` | Redis port |
| `IMAGE_QUEUE_NAME` | `artifact-image-processing` | ARQ queue name (must match worker) |
| `IMAGE_STORAGE_ROOT` | `./data/images` | Root directory for uploaded and processed images |

---

## Application Startup

`app/main.py` defines a lifespan context that runs once at startup before the server begins accepting requests:

```python
@asynccontextmanager
async def lifespan(_app):
    await create_db_and_tables()   # CREATE TABLE IF NOT EXISTS for all models
    async for db in get_session():
        await create_admin_if_needed(db)   # seed admin user if configured
        break
    yield
```

**`create_db_and_tables`** calls `SQLModel.metadata.create_all`, which introspects all imported SQLModel table classes and issues `CREATE TABLE IF NOT EXISTS` for each. No migration framework is used — schema changes require manual migration or a volume wipe in development.

**`create_admin_if_needed`** checks whether a user with `AUTH_ADMIN_EMAIL` already exists. If not, it creates one with role `admin`. If the env vars are empty, nothing happens.

---

## Database

**`app/database.py`** creates a single async SQLAlchemy engine and a session factory:

```python
engine = create_async_engine(settings.database_url, pool_pre_ping=True)

SessionLocal = async_sessionmaker(
    engine,
    class_=AsyncSession,   # SQLModel's async session
    expire_on_commit=False,
)
```

`expire_on_commit=False` means objects remain accessible after a commit without an extra round-trip, which matters for HTMX handlers that build HTML immediately after writing.

`pool_pre_ping=True` issues a cheap `SELECT 1` before using a connection from the pool, preventing errors from stale connections after a database restart.

**Session injection** is handled via a FastAPI dependency:

```python
async def get_session():
    async with SessionLocal() as session:
        yield session
```

Routers declare `db: AsyncSession = Depends(get_session)` to receive a session scoped to the request.

**Query pattern**: All queries use SQLAlchemy's `execute()` + `.scalars()` explicitly rather than SQLModel's `exec()`, which had version-dependent behaviour returning raw `Row` objects instead of model instances:

```python
result = await db.execute(select(User).where(User.email == email))
user = result.scalars().first()
```

---

## Data Models

All models live in `app/models/` and use `SQLModel` with `table=True`. All timestamp columns use `sa_column=Column(DateTime(timezone=True))` so PostgreSQL stores them as `TIMESTAMPTZ`, which is required when using timezone-aware Python datetimes with asyncpg.

### `User` (`users` table)

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | auto-generated |
| `email` | VARCHAR | unique, indexed |
| `password_hash` | VARCHAR | `{salt_hex}:{scrypt_hex}` format |
| `role` | `userrole` enum | `viewer` / `editor` / `admin` |
| `is_active` | BOOLEAN | inactive users cannot log in |
| `created_at` | TIMESTAMPTZ | |
| `updated_at` | TIMESTAMPTZ | |

### `UserSession` (`user_sessions` table)

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `user_id` | UUID FK → `users.id` | CASCADE DELETE |
| `token_hash` | VARCHAR | SHA-256 of the raw token, indexed |
| `expires_at` | TIMESTAMPTZ | server-side expiry check |
| `created_at` | TIMESTAMPTZ | |

### `Artifact` (`artifacts` table)

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `name` | VARCHAR | defaults to empty string |
| `desc` | VARCHAR | description, defaults to empty string |
| `lat` | FLOAT | optional latitude |
| `lng` | FLOAT | optional longitude |
| `parent_id` | UUID FK → `artifacts.id` | self-referential, nullable, CASCADE DELETE |
| `created_at` | TIMESTAMPTZ | |
| `updated_at` | TIMESTAMPTZ | |

Artifacts form a tree via `parent_id`. Deleting a parent cascades to all descendants (enforced at the DB level). The tree is reconstructed in memory from a flat `SELECT *` in `_build_tree()`.

### `ArtifactImage` (`artifact_images` table)

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `artifact_id` | UUID FK → `artifacts.id` | CASCADE DELETE |
| `original_filename` | VARCHAR | original name from the upload |
| `original_mime_type` | VARCHAR | e.g. `image/jpeg` |
| `original_storage_path` | VARCHAR | absolute path on the shared volume |
| `status` | `imagestatus` enum | `queued` → `processing` → `processed` / `failed` |
| `processed_filename` | VARCHAR | filename of the WebP output, nullable |
| `processed_mime_type` | VARCHAR | `image/webp` once processed |
| `width` / `height` | INTEGER | pixel dimensions after processing |
| `error_message` | TEXT | populated on `failed` status |
| `created_at` | TIMESTAMPTZ | |
| `updated_at` | TIMESTAMPTZ | |

---

## Authentication & Sessions

Implementation is in `app/services/auth.py`.

### Password hashing

Passwords are hashed with **scrypt** (Python's `hashlib.scrypt`, n=16384, r=8, p=1) using a 16-byte random salt. The stored value is `{salt_hex}:{derived_key_hex}`. Verification uses `hmac.compare_digest` to prevent timing attacks.

### Login flow

1. `POST /auth/login` receives `email` + `password` as a form body.
2. `auth_service.login()` looks up the user, checks `is_active`, then verifies the password.
3. On success: 32 random bytes are generated as the session token. The raw hex is sent to the browser; the SHA-256 hash is stored in `user_sessions`.
4. The response is a `303 Redirect` to `/` with a `Set-Cookie` header for `pintrail_session` (httpOnly, SameSite=Lax).

### Request authentication

`app/dependencies.py` provides three dependency functions:

- **`get_optional_user`** — reads the `pintrail_session` cookie, hashes it, looks it up in `user_sessions` checking expiry, returns the `User` or `None`.
- **`require_user`** — wraps `get_optional_user`, raises HTTP 401 if no user.
- **`require_role(minimum_role)`** — factory that returns a dependency raising HTTP 403 if the user's role rank is insufficient.

Three pre-built role dependencies are exported for use in routers:

```python
require_viewer = require_role(UserRole.viewer)
require_editor = require_role(UserRole.editor)
require_admin  = require_role(UserRole.admin)
```

### Logout

`POST /auth/logout` deletes the `UserSession` row for the current token and issues a `303 Redirect` to `/` with a `delete_cookie` instruction.

### Session expiry

Expiry is enforced server-side. The query for `get_user_from_token` includes `UserSession.expires_at > now`. The TTL is set by `AUTH_SESSION_TTL_HOURS` (default 24).

---

## Authorization & Role System

Three roles exist with a numeric rank used for hierarchical checks:

| Role | Rank | Permissions |
|---|---|---|
| `viewer` | 1 | Read artifacts and images |
| `editor` | 2 | All viewer permissions + create/update/delete artifacts, upload/delete images |
| `admin` | 3 | All editor permissions + manage users |

A user with a higher rank automatically passes checks for lower ranks. Roles are declared per-endpoint via the dependency injection system — there are no global middleware checks.

---

## Routes Reference

### Frontend (`app/routers/frontend.py`)

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/` | none | Renders `login.html` if unauthenticated, `index.html` if authenticated |
| GET | `/partials/admin` | admin | Returns the `admin_panel.html` partial for HTMX injection |

### Auth (`app/routers/auth.py`, prefix `/auth`)

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/auth/login` | none | Validates credentials, sets session cookie, redirects to `/` |
| POST | `/auth/logout` | none | Deletes session, clears cookie, redirects to `/` |
| GET | `/auth/users` | admin | Returns `user_list.html` partial |
| POST | `/auth/users` | admin | Creates a user, returns updated `user_list.html` partial |
| PATCH | `/auth/users/{user_id}` | admin | Updates role/active/password, returns updated `user_list.html` partial |

### Artifacts (`app/routers/artifacts.py`, prefix `/artifacts`)

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/artifacts/tree` | viewer | Returns `artifact_tree.html` partial (full tree, highlights `selected_id` query param) |
| POST | `/artifacts` | editor | Creates an artifact, returns tree HTML + OOB detail HTML |
| GET | `/artifacts/{id}` | viewer | Returns `artifact_detail.html` partial |
| PATCH | `/artifacts/{id}` | editor | Updates name/desc/lat/lng, returns `<span class="status-saved">` |
| DELETE | `/artifacts/{id}` | editor | Deletes artifact + descendants, returns tree HTML + OOB clear |
| POST | `/artifacts/{id}/images` | editor | Uploads image files, enqueues processing jobs, returns `image_gallery.html` |
| GET | `/artifacts/{id}/images` | viewer | Returns `image_gallery.html` (used for polling) |
| DELETE | `/artifacts/{id}/images/{img_id}` | editor | Deletes image record + files from disk, returns `image_gallery.html` |

### Media (`app/routers/media.py`, prefix `/media`)

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/media/{folder}/{filename}` | none | Serves image files from `IMAGE_STORAGE_ROOT` |

The media route validates both path segments against a strict allowlist regex (`^[a-zA-Z0-9_\-\.]+$`) and resolves the final path against the storage root to prevent path traversal.

---

## Services

### `app/services/auth.py`

Pure async functions with no FastAPI coupling — they take a `db: AsyncSession` and return plain values. This makes them straightforward to test independently.

| Function | Description |
|---|---|
| `create_admin_if_needed(db)` | Seeds the admin user on startup if `AUTH_ADMIN_EMAIL` is set and the user does not exist |
| `login(db, email, password)` | Validates credentials, creates a session, returns the raw token hex or `None` |
| `get_user_from_token(db, token_hex)` | Resolves a session token to a `User`, checking expiry |
| `logout(db, token_hex)` | Deletes the session row |
| `create_user(db, email, password, role)` | Creates a new user |
| `update_user(db, user_id, *, role, is_active, password)` | Partially updates a user |
| `list_users(db)` | Returns all users ordered by `created_at` |

---

## Template Architecture (HTMX)

The portal uses a single-page shell with server-rendered HTML fragments swapped in via HTMX. There is no client-side routing and no JavaScript framework.

### Page structure

```
base.html          — <html>, loads CSS and HTMX CDN script
├── login.html     — shown when not authenticated; standard form POST to /auth/login
└── index.html     — shown when authenticated; contains the portal shell
    ├── #artifact-tree    — sidebar nav (HTMX-loaded on page load)
    ├── #main-content     — detail panel (HTMX-swapped on tree item click)
    └── #image-modal      — fullscreen image overlay (vanilla JS)
```

### Partials

All partials under `templates/partials/` are HTML fragments with no `<html>` wrapper. They are returned by HTMX-targeted endpoints and injected into the DOM.

| Partial | Loaded by | Description |
|---|---|---|
| `artifact_tree.html` | `GET /artifacts/tree` | Renders the full artifact tree recursively using a Jinja2 macro. Active item highlighted by `selected_id`. |
| `artifact_detail.html` | `GET /artifacts/{id}` | The main editing panel — form fields, children list, inline image gallery. |
| `image_gallery.html` | `GET /artifacts/{id}/images` | Image grid with upload dropzone. Self-polling if any image is in `queued` or `processing` status. |
| `user_list.html` | `GET /auth/users`, `POST /auth/users`, `PATCH /auth/users/{id}` | User management table. Each role select and active checkbox sends a PATCH on change. |
| `admin_panel.html` | `GET /partials/admin` | Thin wrapper that includes `user_list.html`. |

### Key HTMX patterns used

**On-load fetch** — The sidebar tree is not rendered into the initial page. Instead it carries `hx-trigger="load"` and fetches itself:

```html
<nav id="artifact-tree" hx-get="/artifacts/tree" hx-trigger="load" hx-swap="innerHTML">
```

**Autosave** — Every form field in `artifact_detail.html` sends a PATCH on input with a 350ms debounce. The response is just a status indicator span — the form itself is not replaced, preventing cursor position loss:

```html
hx-patch="/artifacts/{{ artifact.id }}"
hx-include="closest form"
hx-trigger="input changed delay:350ms"
hx-target="#save-status"
hx-swap="innerHTML"
```

**Out-of-band (OOB) swaps** — When creating or deleting an artifact, two DOM regions need updating simultaneously: the sidebar tree and the main content panel. The server returns both in a single response using HTMX's OOB mechanism:

```python
return HTMLResponse(
    tree_html
    + f'<div id="main-content" hx-swap-oob="innerHTML">{detail_html}</div>'
)
```

The primary response target (`#artifact-tree`) gets `tree_html`; the OOB element updates `#main-content` independently.

**Image status polling** — `image_gallery.html` adds a polling trigger only when images are pending. Once the server re-renders the gallery with all images resolved, the trigger attribute is absent and polling stops automatically:

```html
{% if pending %}
hx-get="/artifacts/{{ artifact_id }}/images"
hx-trigger="every 2s"
hx-swap="outerHTML"
{% endif %}
```

**Dynamic HTMX content (modal)** — The delete button inside the image modal is injected via `innerHTML` in JavaScript. After injection, `htmx.process(body)` must be called to make HTMX recognize the newly added attributes:

```javascript
body.innerHTML = `...<button hx-delete="..." ...>Delete</button>`;
htmx.process(body);
```

---

## Image Upload & Processing Pipeline

The portal handles uploads synchronously but processes images asynchronously.

### Upload (portal)

1. User drops files onto the dropzone or uses the file picker. The form is submitted via HTMX (`hx-encoding="multipart/form-data"`).
2. `POST /artifacts/{id}/images` validates each file's MIME type (must start with `image/`).
3. Each file is written to disk at `IMAGE_STORAGE_ROOT/{artifact_id}/{uuid}_{original_filename}`.
4. An `ArtifactImage` row is inserted with `status = queued`.
5. A job is enqueued to Redis via ARQ: `process_image(str(image_id))`. If Redis is unavailable, the error is logged but the upload is not aborted — the image stays in `queued` state.
6. The updated `image_gallery.html` partial is returned.

### Processing (worker)

The worker (`../worker/`) picks up `process_image` jobs from the queue and:

1. Marks the image `processing`.
2. Opens the original file with Pillow.
3. Converts to RGB/RGBA, resizes to max 2048px (preserving aspect ratio), saves as WebP (quality 85).
4. Updates the `ArtifactImage` row with `status = processed`, `processed_filename`, `width`, `height`.

On failure the row is marked `status = failed` with `error_message` set.

### Storage layout

```
IMAGE_STORAGE_ROOT/
└── {artifact_id}/                  # one directory per artifact
    ├── {uuid}_{original_name}      # raw upload (kept)
    └── processed_{uuid}.webp       # output from worker
```

The portal serves files from this directory via `GET /media/{folder}/{filename}`. The `folder` segment is the artifact UUID string.

---

## Static Files & Media Serving

**CSS** is served as a static file from `app/static/styles.css` via FastAPI's `StaticFiles` mount at `/static`.

The stylesheet uses CSS custom properties for theming (dark warm earth tones) and a grid-based two-column layout (sidebar + main). No CSS framework is used.

**Images** are served by the `media` router rather than `StaticFiles` to enforce path validation and prevent traversal attacks. Both the folder segment and filename segment are validated against a strict regex before the path is resolved.

---

## Docker

The `Dockerfile` builds a production image:

1. Copies `uv` from the official image.
2. Copies `pyproject.toml` and `uv.lock` first, runs `uv sync --frozen --no-dev` to install dependencies into `.venv` — this layer is cached as long as the lock file doesn't change.
3. Copies the `app/` source directory.
4. Runs `uvicorn` directly (no auto-reload) with `--no-access-log`.

```
CMD ["uv", "run", "--no-dev", "uvicorn", "app.main:app",
     "--host", "0.0.0.0", "--port", "8000", "--no-access-log"]
```

The image does **not** bundle the database or Redis — those are provided by the compose network. The `IMAGE_STORAGE_ROOT` path (`/data/images` in compose) must be a shared volume mounted by both the portal and worker containers.

---

## External Dependencies

| Service | Purpose | How configured |
|---|---|---|
| **PostgreSQL 16** | Primary data store | `DB_*` env vars |
| **Redis 7** | ARQ job queue | `REDIS_*` env vars |
| **Worker** (`../worker/`) | Async image processing | Shared DB + Redis + image storage volume |

The portal is not aware of the worker beyond enqueuing jobs. If the worker is down, uploads accumulate in `queued` status and are processed once the worker restarts.
