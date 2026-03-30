# Pintrail — System Architecture

This document describes the complete architecture of the Pintrail system: what each service does, how services communicate, the data model, the request lifecycle, and the deployment topology.

For deeper dives see the per-service documents:
- [`portal/PORTAL.md`](portal/PORTAL.md) — portal internals
- [`worker/WORKER.md`](worker/WORKER.md) — worker internals
- [`deploy/DEPLOY.md`](deploy/DEPLOY.md) — production deployment

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Service Map](#service-map)
3. [Services](#services)
4. [Inter-Service Communication](#inter-service-communication)
5. [Data Model](#data-model)
6. [Request Lifecycles](#request-lifecycles)
7. [Authentication Model](#authentication-model)
8. [Image Processing Pipeline](#image-processing-pipeline)
9. [Storage](#storage)
10. [Deployment Topologies](#deployment-topologies)
11. [Technology Choices](#technology-choices)

---

## System Overview

Pintrail is a private web portal for managing **artifacts** — records that have a name, description, GPS coordinates, a position in a hierarchy, and an attached image gallery. It is built for a small number of authenticated users with differentiated read/write/admin roles.

The system is designed to be self-hosted, run entirely in Docker, and deployed behind a TLS-terminating reverse proxy. There is no public-facing registration flow; all user accounts are created by an admin.

---

## Service Map

```
┌─────────────────────────────────────────────────────────────────┐
│                        Docker Network                           │
│                                                                 │
│   ┌───────────┐        ┌──────────────────────────────────┐    │
│   │   caddy   │──────► │  portal  :8000                   │    │
│   │  :80/:443 │        │  FastAPI / Jinja2 / HTMX         │    │
│   └───────────┘        └────────┬────────────┬────────────┘    │
│        │                        │            │                  │
│   (only service                 │            │                  │
│   with host ports)         ┌────▼────┐  ┌───▼───┐             │
│                             │postgres │  │ redis │             │
│                             │  :5432  │  │ :6379 │             │
│                             └────▲────┘  └───▲───┘             │
│                                  │            │                  │
│                        ┌─────────┴────────────┘                │
│                        │  worker                               │
│                        │  ARQ / Pillow                         │
│                        └───────────────────────────────────────┘
│                                                                 │
│  Shared volume: artifact-images  (portal ←→ worker)            │
└─────────────────────────────────────────────────────────────────┘
```

---

## Services

### portal

**Directory**: `portal/`
**Language**: Python 3.12
**Framework**: FastAPI 0.115, SQLModel, Jinja2, HTMX
**Role**: The only user-facing service. Serves the web UI, enforces authentication and authorization, manages all data, accepts image uploads, and enqueues processing jobs.

The portal owns the database schema. On every startup it runs `SQLModel.metadata.create_all`, creating tables that do not yet exist. It also seeds the initial admin user if `AUTH_ADMIN_EMAIL` / `AUTH_ADMIN_PASSWORD` are set and the account does not already exist.

See [`portal/PORTAL.md`](portal/PORTAL.md) for full internals.

---

### worker

**Directory**: `worker/`
**Language**: Python 3.12
**Framework**: ARQ (job queue), SQLModel, Pillow
**Role**: Consumes image processing jobs from Redis. For each job: marks the image as processing, converts the original upload to WebP (max 2048px), writes the output file, then updates the database record to `processed`. Has no HTTP interface.

The worker does not create or modify the database schema. It depends on the portal having run at least once to create the `artifact_images` and `artifacts` tables.

See [`worker/WORKER.md`](worker/WORKER.md) for full internals.

---

### postgres

**Image**: `postgres:16-alpine`
**Role**: Primary data store. Holds all users, sessions, artifacts, and image metadata. Exposed only on the internal Docker network. The portal is the sole writer of schema DDL; both portal and worker read and write rows.

---

### redis

**Image**: `redis:7-alpine`
**Role**: Job queue broker. The portal enqueues `process_image` jobs; the worker consumes them. ARQ also stores job result metadata in Redis. Exposed only on the internal Docker network.

---

### caddy _(production only)_

**Image**: `caddy:2-alpine`
**Role**: TLS termination and reverse proxy. Provisions Let's Encrypt certificates automatically. Compresses responses with zstd/gzip. Forwards all traffic to the portal on port 8000 over the internal network. The only service that publishes ports (80, 443) to the host.

---

## Inter-Service Communication

| From | To | Protocol | Purpose |
|---|---|---|---|
| Browser | caddy | HTTPS (443) | All user traffic in production |
| Browser | portal | HTTP (8000) | All user traffic in development |
| caddy | portal | HTTP (internal) | Reverse proxy |
| portal | postgres | asyncpg (TCP) | All DB reads and writes |
| portal | redis | TCP | Enqueue `process_image` jobs |
| worker | postgres | asyncpg (TCP) | Read image records, write status/results |
| worker | redis | TCP | Dequeue and acknowledge jobs |
| portal ↔ worker | shared volume | filesystem | Portal writes originals; worker reads originals and writes WebP outputs |

There is no direct portal↔worker HTTP communication. They are decoupled through Redis (job queue) and PostgreSQL (shared state).

---

## Data Model

Four tables. The portal owns the schema; the worker only touches `artifact_images`.

```
users
  id            UUID  PK
  email         VARCHAR  unique
  password_hash VARCHAR  (scrypt: salt:key)
  role          userrole enum  (viewer | editor | admin)
  is_active     BOOLEAN
  created_at    TIMESTAMPTZ
  updated_at    TIMESTAMPTZ

user_sessions
  id          UUID  PK
  user_id     UUID  FK → users.id  CASCADE DELETE
  token_hash  VARCHAR  (SHA-256 of raw token)
  expires_at  TIMESTAMPTZ
  created_at  TIMESTAMPTZ

artifacts
  id         UUID  PK
  name       VARCHAR  default ''
  desc       VARCHAR  default ''
  lat        FLOAT  nullable
  lng        FLOAT  nullable
  parent_id  UUID  FK → artifacts.id  nullable  CASCADE DELETE
  created_at TIMESTAMPTZ
  updated_at TIMESTAMPTZ

artifact_images
  id                    UUID  PK
  artifact_id           UUID  FK → artifacts.id  CASCADE DELETE
  original_filename     VARCHAR
  original_mime_type    VARCHAR
  original_storage_path VARCHAR  (absolute path on shared volume)
  status                imagestatus enum  (queued | processing | processed | failed)
  processed_filename    VARCHAR  nullable
  processed_mime_type   VARCHAR  nullable
  width                 INTEGER  nullable
  height                INTEGER  nullable
  error_message         TEXT     nullable
  created_at            TIMESTAMPTZ
  updated_at            TIMESTAMPTZ
```

### Key relationships

- **Users → UserSessions**: one-to-many. Sessions are deleted when the user is deleted (CASCADE). Sessions also expire server-side by `expires_at`.
- **Artifacts → Artifacts**: self-referential parent/child tree. Deleting a parent cascades to all descendants at the database level.
- **Artifacts → ArtifactImages**: one-to-many. Images are deleted when their artifact is deleted (CASCADE). Image files on disk are deleted by the portal's delete endpoint (not by the DB cascade).

---

## Request Lifecycles

### Unauthenticated page load

```
Browser  GET /
  → portal: no session cookie
  → render login.html
  ← 200 HTML (login form)
```

### Login

```
Browser  POST /auth/login  (email, password)
  → portal: hash password, verify against stored scrypt hash
  → create UserSession row (token_hash stored, raw token returned)
  ← 303 Redirect /  Set-Cookie: pintrail_session=<raw_token>  httpOnly SameSite=Lax
```

### Authenticated page load

```
Browser  GET /  Cookie: pintrail_session=<token>
  → portal: SHA-256(token) → query user_sessions → resolve User
  → render index.html (portal shell with user context)
  ← 200 HTML

Browser  GET /artifacts/tree  (HTMX hx-trigger="load")
  → portal: SELECT * FROM artifacts ORDER BY created_at
  → _build_tree() constructs nested dict structure in memory
  → render partials/artifact_tree.html
  ← 200 HTML fragment → HTMX swaps into #artifact-tree
```

### Artifact autosave

```
Browser  PATCH /artifacts/{id}  (form fields, 350ms after last keystroke)
  → portal: UPDATE artifacts SET name=..., updated_at=now()
  ← 200  <span class="status-saved">Saved</span>
  → HTMX swaps span into #save-status
```

### Image upload

```
Browser  POST /artifacts/{id}/images  (multipart/form-data)
  → portal: write file to IMAGE_STORAGE_ROOT/{artifact_id}/{uuid}_{name}
  → INSERT INTO artifact_images (status='queued', ...)
  → redis.enqueue_job('process_image', image_id)
  → render partials/image_gallery.html  (image shown with spinner)
  ← 200 HTML fragment
```

### Image processing (worker)

```
redis  → worker: dequeue process_image(image_id)
  worker: UPDATE artifact_images SET status='processing'
  worker: Pillow.open(original_path) → convert → resize → save WebP
  worker: UPDATE artifact_images SET status='processed', processed_filename=..., width=..., height=...
```

### Image gallery polling

```
Browser  GET /artifacts/{id}/images  (HTMX every 2s, while pending images exist)
  → portal: SELECT * FROM artifact_images WHERE artifact_id=...
  → if all resolved: render gallery without hx-trigger (polling stops)
  → if pending:      render gallery with hx-trigger="every 2s" (polling continues)
  ← 200 HTML fragment → HTMX outerHTML swaps #image-gallery
```

---

## Authentication Model

- **Transport**: session token in an httpOnly, SameSite=Lax cookie named `pintrail_session`. JavaScript cannot read it.
- **Storage**: only the SHA-256 hash of the token is stored in the database. The raw token exists only in the browser cookie and in transit.
- **Password hashing**: scrypt (n=16384, r=8, p=1) with a 16-byte random salt per user. Stored as `{salt_hex}:{dk_hex}`. Verification uses `hmac.compare_digest` to prevent timing attacks.
- **Expiry**: server-side, checked on every request against `user_sessions.expires_at`. Default TTL is 24 hours (configurable via `AUTH_SESSION_TTL_HOURS`).
- **Role hierarchy**: `viewer (1) < editor (2) < admin (3)`. Higher ranks satisfy lower-rank checks. Enforced per-endpoint via FastAPI dependency injection.

---

## Image Processing Pipeline

```
Upload (portal)                          Process (worker)
───────────────                          ────────────────
User drops file on dropzone
  │
  ▼
MIME type check (must be image/*)
  │
  ▼
Write to disk:
  IMAGE_STORAGE_ROOT/
  └── {artifact_id}/
      └── {uuid}_{original_name}         ◄─ worker reads this
  │
  ▼
INSERT artifact_images
  status = queued
  │
  ▼
redis.enqueue_job('process_image', id)
  │
  ▼
Return image_gallery.html partial        worker picks up job
(spinner shown)                            │
  │                                        ▼
  │                                      UPDATE status = processing
  │                                        │
  │                                        ▼
  │                                      Pillow.open(src)
  │                                      convert colour mode
  │                                      thumbnail to ≤2048px
  │                                      save WebP q=85 method=6
  │                                        │
  │                                        ▼
  │                                      Write to disk:
  │                                        processed_{uuid}.webp   ◄─ portal serves this
  │                                        │
  │                                        ▼
  │                                      UPDATE status = processed
  │                                      processed_filename = ...
  │                                      width, height = ...
  │
  ▼
Browser polls GET /artifacts/{id}/images every 2s
  │
  └── all resolved → gallery re-renders with thumbnails, polling stops
  └── still pending → gallery re-renders with spinners, polling continues
```

---

## Storage

### Database volumes

- `postgres-data` — PostgreSQL data directory. All structured data.
- `redis-data` — Redis RDB snapshot. ARQ job state.

### File volumes

- `artifact-images` — shared between portal and worker, mounted at `/data/images` in both containers.

```
/data/images/
└── {artifact_id}/          one directory per artifact (created by portal on first upload)
    ├── {uuid}_{name}        original upload (portal writes, never deleted by worker)
    └── processed_{uuid}.webp  WebP output (worker writes, portal serves via /media/...)
```

### Media serving

The portal serves image files via `GET /media/{artifact_id}/{filename}`. The handler validates both path segments against a strict regex and resolves the path against the storage root to prevent path traversal. Files are not served as unauthenticated static assets — the media route is unprotected by role but runs through the normal FastAPI request pipeline.

---

## Deployment Topologies

### Development (`compose.yml`)

```
Host ports:  8000 (portal), 5432 (postgres), 6379 (redis)
No Caddy, no TLS, no restart policies.
Hot reload disabled (portal runs in production mode inside Docker;
run uvicorn locally with --reload for live editing).
```

### Production (`compose.prod.yml`)

```
Host ports:  80 (Caddy), 443 (Caddy)
All other services: internal network only.
restart: unless-stopped on all services.
Caddy provisions Let's Encrypt TLS automatically.
Stack managed by systemd via deploy/pintrail.service.
Server path: /opt/pintrail
Env file:    /opt/pintrail/.env
```

See [`deploy/DEPLOY.md`](deploy/DEPLOY.md) for step-by-step instructions.

---

## Technology Choices

| Decision | Choice | Rationale |
|---|---|---|
| Web framework | FastAPI | Async-native, minimal, good dependency injection for auth |
| ORM | SQLModel | Thin SQLAlchemy 2 wrapper with Pydantic-compatible models |
| DB driver | asyncpg | Native async PostgreSQL driver; required for SQLAlchemy async |
| Templates | Jinja2 + HTMX | Server-rendered HTML with partial swaps; no JS framework needed |
| Job queue | ARQ | Lightweight async Redis queue; fits the single-job-type workload |
| Image processing | Pillow | Standard Python image library; WebP encode built-in |
| Reverse proxy | Caddy | Automatic HTTPS with zero certificate management |
| Dependency management | uv | Fast, lock-file-based, per-service virtual environments |
| Containerisation | Docker Compose | Simple multi-service orchestration without Kubernetes overhead |
