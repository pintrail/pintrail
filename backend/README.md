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

## Creating the first admin

Authors are admin-provisioned with no self-service signup, so the first one
comes from the CLI. The password is read from stdin, never from an argument —
process arguments are visible to any user on the host via `ps`.

```sh
cargo run -p pintrail-api -- create-author dean@umass.edu admin
cargo run -p pintrail-api -- reset-password dean@umass.edu   # also revokes sessions
cargo run -p pintrail-api -- help
```

Thereafter admins manage accounts over HTTP at `/admin/authors`.

## Auth model (author tier)

| Property | Choice | Why |
|---|---|---|
| Password hash | argon2id, PHC string | DESIGN.md §2.6 specified scrypt only to preserve legacy hashes; there were none, so both tiers use one hasher. PHC embeds params, so cost can be raised later without invalidating stored hashes. |
| Session token | 256-bit from OS CSPRNG, SHA-256 stored | Only the hash is persisted, so a database leak yields no live sessions. SHA-256 rather than argon2 is correct here: the input is already high-entropy, so there is nothing for a slow hash to defend, and this runs on every request. |
| Cookie | `HttpOnly`, `SameSite=Lax`, `Secure`, `Path=/` | `HttpOnly` keeps an XSS bug from also being session theft. `Lax` blocks the cross-site POST that CSRF needs while still allowing an author to follow a link into the panel. `Secure` is on unless `COOKIE_SECURE=false` for local http. |
| Login failures | One opaque 401 for every cause | Distinct messages for unknown-email, wrong-password, and suspended would enumerate valid accounts. Unknown emails also verify against a dummy hash so response time does not leak the distinction either — measured at 200.0 ms both ways. |
| Role gate | Extractor in the handler signature | `RequireAdmin` in the signature means the check cannot be forgotten: without it the handler has no author value to work with. Under-privileged is 403, unauthenticated is 401. |
| Suspension | Deletes the author's sessions | Otherwise a suspended account keeps working until its cookie happens to expire. Password reset does the same. |
| Self-lockout | Admins cannot demote or suspend themselves | It is the one mistake here with no in-app recovery. A second guard refuses any change leaving zero active admins. |

**Not yet done:** login is not rate-limited, so the argon2 cost is currently the
only brute-force barrier. The rate-limiting infrastructure arrives with comments
in stage 9 and should be applied to `/authors/login` at the same time.

## Auth model (reader tier)

Bearer tokens rather than cookies, 90-day lifetime — an explorer should not be
logged out between campus visits, and the tier cannot edit content.

**Verification gates writing, not reading.** An unverified reader can sign in
and browse; `login` reports `email_verified` so the app can prompt rather than
discovering the limit when a comment fails. Two extractors express this:
`AuthenticatedReader` (signed in) and `VerifiedReader` (signed in and
confirmed), the latter returning a distinct `EmailNotVerified` 403 so the
client can offer "resend" instead of a login screen.

**Registration is not an account-existence oracle.** `POST /readers/register`
returns the same 202 whether or not the address is already registered. A 409 on
duplicate would let anyone test any address for membership — unacceptable on a
public tier. The real owner is not left uninformed: an existing address
receives a "someone tried to register" notice instead of a verification link.
Password reset responds identically for the same reason.

**No unauthenticated endpoint modifies an existing credential.** Re-registering
an address that exists but is unverified re-sends the link and leaves the
stored password untouched. An earlier draft refreshed it, which was an account
takeover: an attacker re-registers a pending address with their own password,
the fresh link lands in the real owner's inbox, the owner clicks it in good
faith, and the account is verified under the attacker's credential. Someone who
genuinely mistyped their password recovers via password reset, which proves
mailbox control first.

| Token | TTL | Notes |
|---|---|---|
| Session (bearer) | 90 days | SHA-256 stored; revoked on logout and on password reset |
| Email verification | 24 hours | Survives a night in a spam folder |
| Password reset | 1 hour | Shorter because a reset link grants account takeover |

Verification and reset tokens are single-use (`consumed_at`), and issuing a new
one supersedes any outstanding token of the same purpose — otherwise every
"resend" click leaves another live link in an inbox. Completing a reset also
marks the address verified, since it proves mailbox control, and revokes every
existing session.

## The sync protocol

`GET /artifacts/sync?since=<version>` returns every artifact whose
`sync_version` exceeds the client's cursor, with coordinates **already
resolved** — the phone never walks the parent chain itself. The response's
`version` is the cursor for next time.

That cursor is the highest version actually returned, not the sequence's
current value. Reading the sequence could skip a row committed by a slower
concurrent transaction holding a lower version.

Three things the protocol has to get right, none of which DESIGN.md §2.4
specifies:

**Deletes are tombstones.** A row that simply vanished would leave every phone
geofencing it forever, since sync only reports what changed. Deletes are soft
(`deleted_at`), bump `sync_version`, and travel to clients flagged `deleted` so
the cache can evict. A *first* sync (`since=0`) omits them — nothing is cached
yet, so shipping every historical deletion is pure waste.

**Deleting a parent deletes the subtree.** A child left behind would inherit
coordinates from a deleted ancestor.

**Moving a parent dirties everything that inherits from it.** This is the
subtle one. An artifact with NULL coordinates reports its ancestor's position,
so moving a building changes the effective location of every room inside it —
without touching those rooms' own `sync_version`. Nothing in the protocol could
then correct the phone, which would geofence rooms at the building's old
coordinates indefinitely. Migration `..._artifact_sync_propagation` adds a
trigger that marks the inheriting descendants dirty. It descends only through
artifacts that actually inherit, since a child with its own coordinates — and
everything beneath it — is unaffected.

## Coordinate resolution

One recursive CTE (`RESOLVED_COORDS_CTE`) propagates coordinates downward from
the roots in a single pass, rather than walking upward per row, which would be
a query per artifact across a whole-campus manifest. The
`artifacts_latlng_paired` constraint guarantees lat and lng are both set or
both null, so coalescing them independently cannot pair one artifact's latitude
with another's longitude.

Detail responses report `lat`/`lng` (the artifact's own, null when inherited),
`effective_lat`/`effective_lng` (resolved), and `location_source_id` (which
ancestor supplied them). An authoring UI needs the distinction — otherwise
"clearing" a coordinate that was never set looks like a broken form.

`PATCH` uses double-`Option` on coordinates: an absent field means "leave
alone", an explicit `null` means "clear this and inherit from the parent".
Collapsing those would make a coordinate impossible to un-set.

## Email delivery

DESIGN.md requires verification but specifies no delivery mechanism, and the
repo has no SMTP configuration. `src/mail.rs` defines the seam: `Mailer` is the
interface, and `LogMailer` (`MAILER=log`, the default) writes messages to the
log instead of sending them, so the flows are exercisable end to end. It warns
loudly at startup so it cannot be deployed by accident. **A real provider is
still required before launch** — add one `impl Mailer` and a branch in
`build_mailer`; no route changes.

`PUBLIC_BASE_URL` builds the links. It is configuration rather than being
derived from the request, because an attacker controls the `Host` header and
could otherwise point a verification link at their own domain.

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
3. ✅ `authors/` — argon2id, cookie sessions, role extractors
4. ✅ `readers/` — registration, email verification, bearer tokens
5. ✅ `artifacts/` — CRUD, coordinate inheritance, `/sync`
6. ⬜ `attachments/` — presigned upload intent, S3
7. ⬜ `pintrail-worker` — `SKIP LOCKED` queue, image→WebP, PDF thumbnails
8. ⬜ `trails/` — stops, visibility, share tokens
9. ⬜ `comments/` — create, list, rate limiting
10. ⬜ `admin/` — minijinja moderation UI
