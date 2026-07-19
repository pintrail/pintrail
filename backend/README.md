# Pintrail backend (Rust)

Implementation of [`docs/DESIGN.md`](../docs/DESIGN.md) — two binaries over one
Postgres database and one S3-compatible bucket.

| Crate | Role |
|---|---|
| [`services/pintrail-api`](services/pintrail-api) | axum HTTP API. Modules per concern (`authors/`, `readers/`, `artifacts/`, `attachments/`, `trails/`, `comments/`, `admin/`), each owning its own tables. |
| [`services/pintrail-worker`](services/pintrail-worker) | Attachment processing. Claims work from Postgres with `FOR UPDATE SKIP LOCKED` — no Redis. |

## Relationship to `pintrail/`

`pintrail/` at the repo root is the **legacy Python system** (portal/artifact/worker)
that this rewrite replaces. It stays deployed and untouched until these services
reach parity. The local Postgres here binds host port **5433** rather than 5432
so both stacks can run side by side during the transition.

## Getting started

```sh
cd backend
cp .env.example .env
docker compose up -d          # postgres + minio, bucket auto-created
cargo run -p pintrail-api     # migrations run automatically on boot
```

In another shell:

```sh
cargo run -p pintrail-worker
```

Verify:

```sh
curl localhost:8080/health        # liveness  -> {"status":"ok",...}
curl localhost:8080/health/ready  # readiness -> {"status":"ready"}
```

`/health` is deliberately independent of the database so an orchestrator does
not restart a healthy API during a database blip; `/health/ready` is the one
that fails when Postgres is unreachable.

## Notes on choices

**Runtime-checked queries.** Queries use `sqlx::query_as` rather than the
compile-time-verified `sqlx::query!` macros, so `cargo build` works without a
live database or a checked-in `.sqlx` cache. If the team later wants
compile-time verification, `cargo sqlx prepare` and a switch to the macros is
the upgrade path.

**Migrations run at startup.** Fine for a single-node deployment. If this ever
runs multiple API replicas, move migrations to an explicit deploy step so
replicas do not race each other.

**`sqlx::migrate!` embeds migrations at compile time.** Adding a `.sql` file
without recompiling means the server applies the set it was last built with
while reporting success. `services/pintrail-api/build.rs` emits a
`rerun-if-changed` on the migrations directory to prevent that.

## Schema deviations from DESIGN.md §2.2

The schema follows the design document except where it was underspecified or
would not survive contact with Postgres. Each of these is a decision worth
revisiting, not an accident:

| Change | Why |
|---|---|
| `artifacts.desc` → `artifacts.description` | `desc` is a reserved SQL keyword requiring quoting at every reference, and `trails` already spells the same concept `description`. |
| Added `artifacts.sync_version` (from a sequence) | `GET /artifacts/sync?since=<version>` needs a monotonic cursor. A sequence beats a timestamp: two rows written in the same microsecond still get distinct ordered versions, and it is immune to clock skew. |
| Added `artifacts.deleted_at` (soft delete) | Without a tombstone, a phone that cached an artifact has no way to learn it was deleted and would display it forever. Deletes bump `sync_version`, so the next sync carries the removal. |
| Added `reader_verification_tokens` | Email verification and password reset need single-use expiring tokens, whose lifecycle differs from a session's — a used token must die immediately. |
| Added `attachments.claimed_at` / `attempts` | Needed by the sweep that re-queues jobs abandoned by a worker that died mid-processing (DESIGN.md §2.5 calls for the sweep but not the columns it requires). |
| Case-insensitive unique email on both identity tables | `Tim@umass.edu` and `tim@umass.edu` are one person; treating them as two accounts is a support ticket at best. |
| Triggers enforce trail owner integrity | `owner_type` + `owner_id` cannot have a declarative foreign key. Triggers validate the owner exists on write and delete a user's trails when the user is deleted — what `ON DELETE CASCADE` would have done. |
| Trigger enforces an acyclic artifact tree | Coordinate inheritance walks up `parent_id`; a cycle would loop forever. Also caps chain depth at 64. |

## Local services

| Service | Address | Credentials |
|---|---|---|
| Postgres | `localhost:5433` | `pintrail` / `pintrail` |
| MinIO API | `localhost:9000` | `pintrail` / `pintrail-dev-secret` |
| MinIO console | `localhost:9001` | same |

## Build stages

Implemented incrementally; see the repo's task list for current position.

1. ✅ Workspace skeleton, config, error type, compose, health endpoints
2. ✅ Migrations — full DESIGN.md §2.2 schema
3. ⬜ `authors/` — scrypt, cookie sessions, role extractors
4. ⬜ `readers/` — registration, email verification, bearer tokens
5. ⬜ `artifacts/` — CRUD, coordinate inheritance, `/sync`
6. ⬜ `attachments/` — presigned upload intent, S3
7. ⬜ `pintrail-worker` — `SKIP LOCKED` queue, image→WebP, PDF thumbnails
8. ⬜ `trails/` — stops, visibility, share tokens
9. ⬜ `comments/` — create, list, rate limiting
10. ⬜ `admin/` — minijinja moderation UI
