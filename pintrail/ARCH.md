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
┌─────────────────────────────────────────────────────────────────────┐
│                          Docker Network                             │
│                                                                     │
│  ┌──────────┐    ┌─────────────────────┐    ┌────────────────────┐ │
│  │  caddy   │──► │  portal  :8000      │──► │ artifact  :8001    │ │
│  │ :80/:443 │    │  FastAPI/Jinja2/HTMX│    │ FastAPI JSON API   │ │
│  └──────────┘    └─────────────────────┘    └────────┬───────────┘ │
│                                                       │             │
│  (only service                                   ┌────▼────┐        │
│  with host ports)                                │postgres │        │
│                                                   │  :5432  │        │
│                                                   └────▲────┘        │
│                                                        │             │
│                  ┌──────────────────────┐  ┌──────────▼──┐         │
│                  │   worker             │  │   redis      │         │
│                  │   ARQ / Pillow       │  │   :6379      │         │
│                  └──────────────────────┘  └──────────────┘         │
│                         ↑ HTTP (artifact API)  ↑ ARQ queue          │
│                         └──────────────────────┘                    │
│                                                                     │
│  Shared volume: artifact-images (artifact writes, worker r/w)       │
│                 portal mounts read-only for /media/ serving         │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Services

### portal

**Directory**: `portal/`
**Language**: Python 3.12
**Framework**: FastAPI 0.115, SQLModel, Jinja2, HTMX
**Role**: The only user-facing service. Serves the web UI, enforces authentication and authorization, and proxies all artifact data operations through the artifact service HTTP API.

The portal owns the **user database schema** only — `users`, `user_sessions`, `userrole`. On every startup it runs `SQLModel.metadata.create_all` for those tables and seeds the initial admin user if `AUTH_ADMIN_EMAIL` / `AUTH_ADMIN_PASSWORD` are set.

Artifact data is fetched and mutated exclusively by calling the artifact service REST API using a shared `httpx.AsyncClient` (created in the FastAPI lifespan).

See [`portal/PORTAL.md`](portal/PORTAL.md) for full internals.

---

### artifact

**Directory**: `artifact/`
**Language**: Python 3.12
**Framework**: FastAPI 0.115, SQLModel
**Role**: Sole owner of artifact data. Provides a JSON REST API for CRUD operations on `artifacts` and `artifact_images`. Handles image file storage on the shared volume and enqueues image processing jobs to Redis.

The artifact service owns the **artifact database schema** — `artifacts`, `artifact_images`. It is the only service that connects to these tables. On startup it runs `create_all` for those two tables.

All requests require an `X-API-Key` header (shared secret between portal, worker, and artifact service).

---

### worker

**Directory**: `worker/`
**Language**: Python 3.12
**Framework**: ARQ (job queue), Pillow
**Role**: Consumes image processing jobs from Redis. For each job: fetches the image record from the artifact service, marks it as `processing`, converts the original upload to WebP (max 2048px), writes the output file, then updates the record to `processed` via the artifact service API. Has no HTTP server port.

The worker has no direct database access. All reads and writes go through the artifact service HTTP API.

See [`worker/WORKER.md`](worker/WORKER.md) for full internals.

---

### postgres

**Image**: `postgres:16-alpine`
**Role**: Primary data store. Holds all users, sessions, artifacts, and image metadata. Exposed only on the internal Docker network.

- **Schema owners**: portal owns `users`/`user_sessions`/`userrole`; artifact service owns `artifacts`/`artifact_images`.
- Only portal and artifact service connect directly.

---

### redis

**Image**: `redis:7-alpine`
**Role**: Job queue broker. The artifact service enqueues `process_image` jobs; the worker consumes them. ARQ also stores job result metadata in Redis. Exposed only on the internal Docker network.

---

### caddy _(production only)_

**Image**: `caddy:2-alpine`
**Role**: TLS termination and reverse proxy. Provisions Let's Encrypt certificates automatically. Compresses responses with zstd/gzip. Forwards all traffic to the portal on port 8000 over the internal network. The only service that publishes ports (80, 443) to the host.

The artifact service is internal only — Caddy does not expose it.

---

## Inter-Service Communication

| From | To | Protocol | Purpose |
|---|---|---|---|
| Browser | caddy | HTTPS (443) | All user traffic in production |
| Browser | portal | HTTP (8000) | All user traffic in development |
| caddy | portal | HTTP (internal) | Reverse proxy |
| portal | postgres | asyncpg (TCP) | User/session DB reads and writes |
| portal | artifact | HTTP (internal, X-API-Key) | All artifact and image CRUD |
| artifact | postgres | asyncpg (TCP) | Artifact/image DB reads and writes |
| artifact | redis | TCP | Enqueue `process_image` jobs |
| worker | artifact | HTTP (internal, X-API-Key) | Fetch image records, update status |
| worker | redis | TCP | Dequeue and acknowledge jobs |
| artifact ↔ worker | shared volume | filesystem | Artifact writes originals; worker reads originals and writes WebP outputs |
| portal ↔ artifact-images | shared volume | filesystem (read) | Portal serves `/media/` files |

There is no direct portal↔worker communication. They are decoupled through the artifact service (data) and Redis (jobs).

---

## Data Model

Five tables across two schema owners.

**Portal owns** (user tables):

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
```

**Artifact service owns** (artifact tables):

```
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

- **Users → UserSessions**: one-to-many. Sessions are deleted when the user is deleted (CASCADE).
- **Artifacts → Artifacts**: self-referential parent/child tree. Deleting a parent cascades to all descendants at the database level. The artifact service also cleans up the filesystem directory.
- **Artifacts → ArtifactImages**: one-to-many. Images are deleted when their artifact is deleted (CASCADE). Image files on disk are deleted by the artifact service delete endpoints.

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
  → portal  GET /artifacts  → artifact service
  ← JSON list of all artifacts
  → _build_tree() constructs nested dict structure in memory
  → render partials/artifact_tree.html
  ← 200 HTML fragment → HTMX swaps into #artifact-tree
```

### Artifact autosave

```
Browser  PATCH /artifacts/{id}  (form fields, 350ms after last keystroke)
  → portal  PATCH /artifacts/{id}  → artifact service  (JSON body)
  ← updated artifact JSON
  ← 200  <span class="status-saved">Saved</span>
  → HTMX swaps span into #save-status
```

### Image upload

```
Browser  POST /artifacts/{id}/images  (multipart/form-data)
  → portal: read files into memory, forward as multipart
  → artifact service: write files to IMAGE_STORAGE_ROOT/{artifact_id}/{uuid}_{name}
  → artifact service: INSERT INTO artifact_images (status='queued', ...)
  → artifact service: redis.enqueue_job('process_image', image_id)
  ← JSON list of images for artifact
  → portal: render partials/image_gallery.html  (image shown with spinner)
  ← 200 HTML fragment
```

### Image processing (worker)

```
redis  → worker: dequeue process_image(image_id)
  worker  GET /images/{image_id}  → artifact service
  ← JSON image record (original_storage_path, etc.)
  worker  PATCH /images/{image_id} {"status": "processing"}  → artifact service
  worker: Pillow.open(original_path) → convert → resize → save WebP
  worker  PATCH /images/{image_id} {"status": "processed", ...}  → artifact service
```

### Image gallery polling

```
Browser  GET /artifacts/{id}/images  (HTMX every 2s, while pending images exist)
  → portal  GET /artifacts/{id}/images  → artifact service
  ← JSON list of images
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

### Internal service authentication

All requests from portal and worker to the artifact service require an `X-API-Key` header. The artifact service validates this using `hmac.compare_digest` against the `API_KEY` environment variable. Requests with a missing or incorrect key receive a 403 response.

---

## Image Processing Pipeline

```
Upload (portal → artifact service)         Process (worker)
───────────────────────────────            ────────────────
User drops file on dropzone
  │
  ▼
Portal proxies multipart upload
  → artifact service
  │
  ▼
MIME type check (must be image/*)
  │
  ▼
Artifact service writes to disk:
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
Return image list JSON                    worker picks up job
Portal renders image_gallery.html          │
(spinner shown)                            ▼
  │                               PATCH /images/{id} {status: processing}
  │                                        │
  │                                        ▼
  │                                      Pillow.open(src)
  │                                      convert colour mode
  │                                      thumbnail to ≤2048px
  │                                      save WebP q=85 method=6
  │                                        │
  │                                        ▼
  │                                      Write to disk:
  │                                        processed_{uuid}.webp  ◄─ portal serves this
  │                                        │
  │                                        ▼
  │                               PATCH /images/{id} {status: processed, ...}
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

- `artifact-images` — mounted at `/data/images` in the artifact service and worker containers (read/write). Also mounted read-only in the portal container for `/media/` serving.

```
/data/images/
└── {artifact_id}/          one directory per artifact (created by artifact service on first upload)
    ├── {uuid}_{name}        original upload (artifact service writes)
    └── processed_{uuid}.webp  WebP output (worker writes, portal serves via /media/...)
```

### Media serving

The portal serves image files via `GET /media/{artifact_id}/{filename}`. The handler validates both path segments against a strict regex and resolves the path against the storage root to prevent path traversal. Files are not served as unauthenticated static assets — the media route runs through the normal FastAPI request pipeline.

---

## Deployment Topologies

### Development (`compose.yml`)

```
Host ports:  8000 (portal), 8001 (artifact), 5432 (postgres), 6379 (redis)
No Caddy, no TLS, no restart policies.
ARTIFACT_API_KEY defaults to 'dev-api-key'.
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
| HTTP client | httpx | Async-native, first-class support for multipart and connection pooling |
| Job queue | ARQ | Lightweight async Redis queue; fits the single-job-type workload |
| Image processing | Pillow | Standard Python image library; WebP encode built-in |
| Internal auth | HMAC API key | Simple shared secret; sufficient for internal Docker network traffic |
| Reverse proxy | Caddy | Automatic HTTPS with zero certificate management |
| Dependency management | uv | Fast, lock-file-based, per-service virtual environments |
| Containerisation | Docker Compose | Simple multi-service orchestration without Kubernetes overhead |
