# Pintrail: Platform Design Document

*A location-triggered artifact discovery and trail-building platform for the UMass Amherst
campus, extensible to museums, campus tours, and similar walking experiences.*

---

# PART 1 — Overview for Everyone

*This section assumes no technical background. It explains what Pintrail is, who uses it,
and how the pieces fit together, before Part 2 gets into implementation detail.*

## 1.1 What Pintrail is

Pintrail helps someone walking around a physical space — starting with the UMass Amherst
campus — discover interesting things nearby, learn about them, and optionally follow a
curated path between them. The two core ideas:

- **Artifacts**: things worth knowing about. A building, a room inside a building, a
  painting on a wall, a rooftop solar array — each is an "artifact" with a name,
  description, location, and a set of media (photos, audio, video, PDFs) that help someone
  understand what it is and why it matters.
- **Trails**: a curated sequence of artifacts to visit in order, with its own theme. A
  "sustainability tour," a "museum highlights tour," and an "admitted-students walk" are all
  just trails — the same underlying thing, pointed at different artifacts, with different
  framing.

The first use case is sustainability (solar rooftops, green buildings, etc.), but the system
is intentionally general — trails and artifacts aren't sustainability-specific, so the same
platform can host a museum tour or an admissions walk without new engineering.

## 1.2 The three kinds of people who use it

| Who | What they do | How many |
|---|---|---|
| **Authors** | Create and edit artifacts: write descriptions, upload photos/audio/video/PDFs, organize the building → room → item hierarchy | A small, trusted, admin-provisioned group (faculty, staff, students working on this) |
| **Explorers** (regular users) | Walk around campus with the phone app, get notified when near an artifact, read about it, view its media, comment on it, and build their own trails to share | Potentially tens of thousands, self-service signup |
| **Admins** | Manage author accounts, moderate comments and user-created trails if something's reported | A very small group |

This is a deliberate three-tier design: authors and admins are a small trusted circle who
log in the traditional way; explorers are the general public, signing up themselves the way
they'd sign up for any consumer app.

## 1.3 What an artifact actually is

An artifact can be almost anything physical worth explaining, and artifacts can nest inside
each other:

```
Elm Building                        ← a building (artifact)
 ├── Room 214, Solar Control Room    ← a room inside it (artifact)
 ├── Rooftop Solar Array             ← a feature of the building (artifact)
 └── Lobby Mural                     ← an object inside it (artifact)
      "Community Roots" (2019)
```

Each artifact carries:
- A **name** and **description** (text).
- A **kind** — building, room, artwork, installation, rooftop, etc.
- A **location** — most artifacts have GPS coordinates. Ones that don't (a room, a painting
  on an interior wall — GPS doesn't work well indoors) automatically use their parent
  building's location instead, so "you're near this" still works.
- **Attachments** — any number of photos, an audio clip, a short video, or a PDF, each
  helping tell the artifact's story.

## 1.4 What a trail is

A trail is an ordered list of artifacts with a title and description, meant to be followed
in sequence. Two trails can include the same artifact — a building might appear on both the
sustainability tour (for its solar roof) and the admissions tour (for its history) with
different framing on each.

**Example trail — "Campus Sustainability Walk" (curated by the sustainability office):**
1. Start at the Recycling & Composting Center — intro to campus waste programs
2. Elm Building Rooftop Solar Array — how the panels work, annual output
3. Rain Garden behind the Life Sciences Building — stormwater management
4. Dining Commons Composting Station — food waste diversion program

**Example trail — "Student-made: My Favorite Quiet Spots"** (built by a regular user,
shared with two friends via a link):
1. Reading nook on the 3rd floor of the library
2. Bench by the pond behind the Fine Arts Center
3. Rooftop garden on the Life Sciences building

Both are the same underlying "trail" concept. The first is admin-curated and public; the
second is user-created and shared privately with a link.

## 1.5 How discovery works

Walking around campus, the phone periodically checks its own GPS position against a list of
known artifact locations (downloaded once, refreshed occasionally — this works even with
patchy signal, since the phone isn't constantly asking the server "what's near me"). When
it's near a building with an artifact inside, it can notify: *"You're near Elm Building —
tap to see what's here."*

For version one, "near" means "near the building" — indoor GPS isn't reliable enough to
distinguish a specific room or painting yet. A future phase can add small Bluetooth beacons
near specific indoor artifacts (a coin-cell-powered device that just broadcasts "I'm artifact
#214" — see §2.7) for that finer precision, without changing anything about how artifacts or
trails work today.

## 1.6 System diagram (non-technical view)

```
                    ┌─────────────────────────┐
                    │      Your Phone          │
                    │  (iPhone or Android)     │
                    │                          │
                    │  • Notices you're near   │
                    │    an artifact           │
                    │  • Shows artifact info,  │
                    │    photos, audio, video  │
                    │  • Lets you comment       │
                    │  • Lets you build trails  │
                    └────────────┬─────────────┘
                                 │  (over the internet)
                                 ▼
                    ┌─────────────────────────┐
                    │     Pintrail Server      │
                    │                          │
                    │  • Keeps track of every  │
                    │    artifact and trail    │
                    │  • Checks who's allowed  │
                    │    to do what            │
                    │  • Converts uploaded     │
                    │    photos/audio/video    │
                    │    into phone-friendly   │
                    │    formats               │
                    └────────────┬─────────────┘
                                 │
              ┌──────────────────┴───────────────────┐
              ▼                                       ▼
   ┌─────────────────────┐                ┌─────────────────────┐
   │   The Database       │                │   The Media Store    │
   │  All artifact/trail/  │                │  Every photo, audio   │
   │  comment/user records │                │  clip, video, PDF     │
   └─────────────────────┘                └─────────────────────┘
```

## 1.7 App wireframes (illustrative, low-fidelity)

**Nearby / map view** — the home screen while walking around:

```
┌───────────────────────────────┐
│ ≡   Pintrail          🔔 3     │
├───────────────────────────────┤
│                                │
│        🗺️  [ MAP VIEW ]        │
│                                │
│     📍you        📍            │
│              📍     📍         │
│         📍                     │
│                                │
├───────────────────────────────┤
│ 🔔 You're near Elm Building    │
│    Tap to see 3 artifacts here │
├───────────────────────────────┤
│ [ Nearby ]  [ Trails ]  [ Me ] │
└───────────────────────────────┘
```

**Artifact detail view** — after tapping a notification or map pin:

```
┌───────────────────────────────┐
│ ←  Rooftop Solar Array         │
├───────────────────────────────┤
│  [ photo 1 ] [ photo 2 ] [›]   │
├───────────────────────────────┤
│ Elm Building · Rooftop         │
│                                │
│ Installed 2021, this 40kW      │
│ array provides roughly 12%     │
│ of the building's annual...    │
│                                │
│  ▶ 0:00 ────────────── 1:42    │
│    "Listen: how it works"      │
│                                │
│  📄 Spec sheet.pdf              │
├───────────────────────────────┤
│ 💬 Comments (4)      [ Add + ] │
│  "Cool to see this in action!" │
│   — jstudent42                 │
├───────────────────────────────┤
│ [ Nearby ]  [ Trails ]  [ Me ] │
└───────────────────────────────┘
```

**Trail view:**

```
┌───────────────────────────────┐
│ ←  Campus Sustainability Walk  │
│    Curated by UMass Sustain.  │
├───────────────────────────────┤
│ ● 1  Recycling & Compost Ctr   │
│ │      ✓ visited                │
│ ● 2  Elm Rooftop Solar Array   │
│ │      📍 you are here          │
│ ○ 3  Rain Garden — Life Sci.   │
│ ○ 4  Dining Commons Compost    │
├───────────────────────────────┤
│        [ Start Walking ]       │
├───────────────────────────────┤
│ [ Nearby ]  [ Trails ]  [ Me ] │
└───────────────────────────────┘
```

**Build-your-own-trail view:**

```
┌───────────────────────────────┐
│ ←  New Trail                   │
├───────────────────────────────┤
│ Title: [My Favorite Spots   ]  │
│ Visibility:                    │
│  ○ Only me   ● Share by link   │
│  ○ Public — anyone can find it │
├───────────────────────────────┤
│ Stops (drag to reorder)        │
│  ≡ 1. Library reading nook  ✕  │
│  ≡ 2. Pond bench            ✕  │
│  ≡ 3. Rooftop garden        ✕  │
│      [ + Add a stop ]          │
├───────────────────────────────┤
│         [ Save Trail ]         │
│      [ Copy Share Link ]       │
└───────────────────────────────┘
```

## 1.8 Glossary

| Term | Meaning |
|---|---|
| Artifact | A single thing that can be visited/learned about — a building, room, object, or feature |
| Attachment | A piece of media on an artifact — image, audio, video, or PDF |
| Trail | An ordered sequence of artifacts, curated by an admin/author or built by any explorer |
| Author | A trusted account holder who creates/edits artifacts |
| Explorer | A regular app user — reads artifacts, comments, builds trails |
| Admin | Manages author accounts and moderates comments/trails |
| Geofencing | The phone detecting "I am near X" using GPS, without needing to ask the server constantly |
| BLE beacon | A small Bluetooth device that broadcasts an ID, used later for room-level indoor precision (see §2.7) |

---

# PART 2 — Technical Architecture

## 2.1 Services

Two Rust binaries, sharing one Postgres database:

```
┌───────────────────────────────────────────────────────────────────┐
│                            Docker host                            │
│                                                                     │
│   ┌────────┐    ┌───────────────────────┐     ┌─────────────────┐ │
│   │ Caddy  │───►│     pintrail-api      │────►│    Postgres      │ │
│   │ TLS    │    │  (axum, one binary)   │     │                  │ │
│   └────────┘    │                        │     │  all schemas:    │ │
│                 │  modules:              │     │  authors, readers│ │
│                 │   authors/  (cookie)   │     │  artifacts,      │ │
│                 │   readers/  (bearer)   │     │  attachments,    │ │
│                 │   artifacts/           │     │  trails, stops,  │ │
│                 │   attachments/         │     │  comments        │ │
│                 │   trails/              │     └────────▲─────────┘ │
│                 │   comments/            │              │           │
│                 │   admin/    (cookie)   │              │ polls     │
│                 └──────────┬─────────────┘              │ SKIP      │
│                             │                             │ LOCKED   │
│                             │ presigned upload/           │           │
│                             │ download URLs         ┌─────┴──────┐   │
│                             ▼                        │pintrail-   │   │
│                 ┌───────────────────────┐            │worker      │   │
│                 │   Object storage       │◄──────────┤(N replicas)│   │
│                 │  (S3-compatible:        │  reads/    │            │   │
│                 │   S3, R2, or MinIO)     │  writes    │ image/     │   │
│                 │  originals + processed  │  media     │ audio/     │   │
│                 └───────────────────────┘            │ video/pdf  │   │
│                                                        └────────────┘   │
└───────────────────────────────────────────────────────────────────┘
                    ▲                              ▲
                    │ HTTPS, bearer token           │ HTTPS, cookie (login,
              ┌─────┴──────┐                        │ session), used only
              │  iOS /      │                        │ for the small admin
              │  Android    │                  ┌─────┴──────┐
              │  app        │                  │  Web admin  │
              └─────────────┘                  │  (browser)  │
                                                └─────────────┘
```

Why two binaries and not more: the earlier design considered three Python-style services
(portal/artifact/worker). Collapsing portal+artifact into one `pintrail-api` binary removes
an internal network hop and a shared-secret auth scheme that existed only because Python's
process model made "different FastAPI app" the natural unit of separation — Rust doesn't
need that, module boundaries (`authors/`, `readers/`, etc.) give the same separation of
concerns without a second process. `pintrail-worker` stays separate because image/audio/video
processing is CPU-bound background work that benefits from being independently restartable
and horizontally scalable without touching the request-serving path.

## 2.2 Data model

Three identity tables, kept genuinely separate rather than one role enum, because authors
and explorers are different products with different auth flows:

```sql
-- Small, trusted, admin-provisioned group
authors
  id              UUID PK
  email           VARCHAR UNIQUE
  password_hash   VARCHAR         -- scrypt, "salt_hex:dk_hex"
  role            author_role     -- viewer | editor | admin
  is_active       BOOLEAN
  created_at, updated_at TIMESTAMPTZ

author_sessions
  id              UUID PK
  author_id       UUID FK -> authors.id CASCADE
  token_hash      VARCHAR         -- sha256, cookie-based
  expires_at      TIMESTAMPTZ
  created_at      TIMESTAMPTZ

-- Large, self-service group
readers
  id              UUID PK
  email           VARCHAR UNIQUE
  email_verified  BOOLEAN DEFAULT false
  password_hash   VARCHAR
  is_active       BOOLEAN         -- admin can suspend
  created_at, updated_at TIMESTAMPTZ

reader_sessions   -- or reader_tokens: long-lived bearer tokens, not cookies
  id              UUID PK
  reader_id       UUID FK -> readers.id CASCADE
  token_hash      VARCHAR
  expires_at      TIMESTAMPTZ
  created_at      TIMESTAMPTZ
```

```sql
-- Owned by authors
artifacts
  id              UUID PK
  kind            artifact_kind    -- building | room | artwork | installation | rooftop | other
  name            VARCHAR DEFAULT ''
  desc            TEXT DEFAULT ''
  lat, lng        FLOAT NULLABLE   -- NULL means "inherit from parent" (see §2.4)
  parent_id       UUID FK -> artifacts.id NULLABLE CASCADE
  beacon_id       VARCHAR NULLABLE -- reserved for v2 BLE precision, unused in v1
  created_at, updated_at TIMESTAMPTZ

-- Owned by authors, replaces the old "images-only" table
attachments
  id                     UUID PK
  artifact_id            UUID FK -> artifacts.id CASCADE
  kind                   attachment_kind   -- image | audio | video | pdf | text
  position               INT              -- display order within the artifact
  caption                VARCHAR NULLABLE
  original_filename      VARCHAR
  original_mime_type     VARCHAR
  original_storage_key   VARCHAR          -- object storage key, not a local path
  status                 processing_status -- queued | processing | processed | failed
  processed_storage_key  VARCHAR NULLABLE
  processed_mime_type    VARCHAR NULLABLE
  width, height          INT NULLABLE      -- image/video
  duration_seconds       FLOAT NULLABLE    -- audio/video
  error_message          TEXT NULLABLE
  created_at, updated_at TIMESTAMPTZ
```

```sql
-- Owned jointly: curated by authors, or built by readers
trails
  id              UUID PK
  title           VARCHAR
  description     TEXT DEFAULT ''
  owner_type      owner_type       -- author | reader
  owner_id        UUID             -- FK to authors.id or readers.id depending on owner_type
  visibility      trail_visibility -- private | unlisted | public
  share_token     VARCHAR UNIQUE NULLABLE  -- opaque token for unlisted sharing links
  created_at, updated_at TIMESTAMPTZ

trail_stops
  id              UUID PK
  trail_id        UUID FK -> trails.id CASCADE
  artifact_id     UUID FK -> artifacts.id CASCADE
  position         INT
  note            TEXT NULLABLE     -- the trail creator's own gloss on this stop

-- Owned by readers, moderated by admins
comments
  id              UUID PK
  artifact_id     UUID FK -> artifacts.id CASCADE
  reader_id       UUID FK -> readers.id CASCADE
  body            TEXT
  status          comment_status   -- visible | hidden | flagged
  created_at, updated_at TIMESTAMPTZ
```

**Why `owner_type` + `owner_id` instead of two nullable foreign keys on `trails`:** it makes
"exactly one owner, and I know which table to join against" explicit, versus two nullable
FKs where you'd need a check constraint anyway to enforce exactly one is set. A partial
index on `(owner_type, owner_id)` covers "show me my trails" for either kind of owner.

**Why `lat`/`lng` nullable with inheritance instead of always-required:** rooms and
paintings genuinely don't have their own useful GPS coordinate — indoor GPS is unreliable,
and the "real" location is "wherever the parent building is." Resolving this at read time
(walk up `parent_id` until a non-null coordinate is found) keeps write-side logic simple:
authors just don't fill in lat/lng for indoor child artifacts.

## 2.3 Module boundaries inside `pintrail-api`

```
services/pintrail-api/
  src/
    main.rs
    config.rs
    state.rs                 # AppState { db: PgPool, storage: S3Client, settings }
    error.rs
    authors/
      model.rs                # Author, AuthorRole (viewer<editor<admin), AuthorSession
      auth.rs                 # scrypt hash/verify, cookie session issuance
      extractors.rs           # CookieAuth<Author>, RequireAuthorRole(min)
      routes.rs                # login/logout, admin-only author management
    readers/
      model.rs                # Reader, ReaderSession
      auth.rs                 # registration, email verification, password reset, bearer tokens
      extractors.rs           # BearerAuth<Reader>
      routes.rs                # POST /readers/register, /readers/login, /readers/verify-email, ...
    artifacts/
      model.rs, routes.rs      # CRUD (author-write, reader-read), /sync manifest endpoint
    attachments/
      model.rs, routes.rs      # upload intent -> presigned URL, list/delete, status polling
    trails/
      model.rs, routes.rs      # create/edit (author or reader depending on owner_type),
                                # visibility + share-token resolution, stop reordering
    comments/
      model.rs, routes.rs      # create (rate-limited), list, reader can delete own
    admin/
      routes.rs                # author account management (existing pattern),
                                # comment moderation, trail takedown
      templates/                # minijinja: login.html, partials/author_list.html,
                                 # partials/comment_queue.html, partials/trail_takedown.html
```

Each module owns its own tables the same way the original Python services did by process —
here it's enforced by convention (only `authors::model` touches the `authors` table) rather
than a network boundary, which is the deliberate trade made back in §"do we even need
separate services" — you lose nothing in practice for a single-node deployment, and gain
one fewer network hop on every request.

## 2.4 Geofencing / proximity design

**v1 (building-level, GPS-based):**
- Every artifact resolves to an effective `(lat, lng)`: its own if set, else its nearest
  ancestor's, walking up `parent_id`.
- The phone app calls `GET /artifacts/sync?since=<version>` periodically (on app open, and
  every few hours in the background) and caches the full list of artifact IDs + effective
  coordinates + basic metadata locally.
- Proximity detection runs **on-device**: iOS `CLLocationManager` region monitoring /
  Android `Geofencing API`, registered against the cached coordinate list. This means
  notifications work even with poor connectivity, and the server isn't doing any real-time
  "who's near what" computation.
- Heavier content (attachment media) is fetched lazily, only when the user opens a specific
  artifact — not pre-downloaded for the whole campus.

**v2 (room-level, BLE beacon-based):** a BLE beacon is a small, battery-powered Bluetooth
device that continuously broadcasts a fixed identifier (no pairing/connection needed). For
an indoor artifact where "the whole building" is too coarse, a beacon placed near it lets the
phone key off "I detected beacon #1234" instead of GPS distance. This is why `beacon_id` is
already reserved (nullable, unused) on `artifacts` — when this ships, the app's proximity
logic branches per-artifact: use GPS distance if `beacon_id` is null, use beacon detection if
it's set. No schema migration needed when this phase starts, just client logic and
provisioning beacons physically.

## 2.5 Attachment processing pipeline

`pintrail-worker` dispatches by `attachment.kind` instead of assuming images:

| Kind | Processing | Crates |
|---|---|---|
| `image` | HEIC/JPEG/PNG decode → resize (max 2048px, matching today) → WebP encode q=85 | `image`, plus `libheif-rs` for iPhone HEIC captures |
| `audio` | Transcode to a single consistent format (e.g. AAC/M4A) for playback consistency | `symphonia` (decode) + shelling out to `ffmpeg`, or a pure-Rust encoder if one covers the target codec adequately |
| `video` | Transcode to H.264/MP4 + extract a thumbnail frame | `ffmpeg` (via `ffmpeg-next` bindings or subprocess) — this is the heaviest new dependency in the whole system, worth scoping as its own spike before committing |
| `pdf` | Store as-is; render page 1 to an image for a thumbnail | `pdfium-render` or subprocess via `pdftoppm` |
| `text` | No processing — it's just a text block; not really an "attachment" in the media sense, could alternately live as a `body` field directly on a `text`-kind attachment row for uniformity |

**Queue mechanism**: Postgres, not Redis — `attachments.status` is already a queue column.
Worker polls with:
```sql
SELECT * FROM attachments
WHERE status = 'queued'
ORDER BY created_at
FOR UPDATE SKIP LOCKED
LIMIT 10;
```
`SKIP LOCKED` lets multiple worker replicas poll the same table with zero coordination and no
double-processing — this is the exact pattern Postgres added `SKIP LOCKED` for. No Redis
container, no separate queue-durability story to design; crash recovery is "the row is still
`queued` or stuck on `processing`," and a periodic sweep can re-queue anything stuck in
`processing` past a timeout.

**Storage**: attachments read/write directly to S3-compatible object storage rather than a
local Docker volume — necessary once uploads are coming from thousands of phones rather than
a handful of authors on one machine. `pintrail-api` issues presigned upload URLs so the phone
uploads the original directly to the bucket (not proxied through the API), and the worker
writes processed output back to the same bucket. Media is served to the app via presigned
(time-limited, signed) download URLs rather than a bearer-token-protected route — this is
what most mobile-backed APIs do, and sidesteps needing every native image/video loader to
attach an `Authorization` header.

## 2.6 Auth summary

| Tier | Transport | Mechanism | Notes |
|---|---|---|---|
| Authors | httpOnly cookie (web admin + any future authoring tool) | scrypt password hash, sha256 session token | Small group, admin-provisioned, no self-service signup |
| Readers | `Authorization: Bearer <token>` (mobile app) | scrypt or argon2 password hash (argon2 fine here — no legacy hashes to preserve for this new tier), sha256 token, email verification required before posting comments/trails | Self-service registration, rate-limited |
| Admin routes | httpOnly cookie, `RequireAuthorRole(Admin)` | same as authors, gated by role | Author/comment/trail moderation UI |

## 2.7 API surface (representative, not exhaustive)

```
# Readers (mobile, bearer)
POST   /readers/register
POST   /readers/verify-email
POST   /readers/login                     -> { token }
GET    /artifacts/sync?since=<version>    -> manifest: id, kind, effective lat/lng, name, parent_id
GET    /artifacts/{id}                    -> full detail incl. attachments (presigned URLs)
GET    /trails/{id}                       -> stops in order
GET    /trails?owner=me
POST   /trails                            -> create (owner_type=reader)
PATCH  /trails/{id}                        -> edit stops/visibility
GET    /trails/shared/{share_token}        -> resolve an unlisted share link
POST   /artifacts/{id}/comments            -> rate-limited
GET    /artifacts/{id}/comments

# Authors (cookie, admin panel or future authoring client)
POST   /authors/login
POST   /artifacts                          -> create/edit, editor+
POST   /artifacts/{id}/attachments/upload-intent   -> presigned PUT URL
PATCH  /attachments/{id}                    -> caption/position edits
POST   /trails                              -> create (owner_type=author), admin/editor curated

# Admin (cookie, admin role only)
GET    /admin/authors, POST/PATCH …          -> author account management
GET    /admin/comments?status=flagged         -> moderation queue
PATCH  /admin/comments/{id}                    -> hide/restore
DELETE /admin/trails/{id}                      -> takedown
```

## 2.8 Rollout / v1 cut line

**v1:**
- Artifacts with nesting + kind + coordinate inheritance.
- Attachments: image + PDF (skip audio/video transcoding complexity initially unless
  content is ready — text/image/PDF cover most sustainability content on day one).
- Building-level GPS proximity, on-device geofencing.
- Reader signup/login, comments, rate limiting.
- Reader-built trails (private/unlisted/public + share links).
- Admin panel: author management + comment/trail moderation.
- Object storage for all media, presigned upload/download.

**v2 candidates** (each independently addable without schema upheaval, per the design
choices above):
- Audio/video attachments (the `ffmpeg` pipeline).
- BLE beacon-based indoor precision (`beacon_id` already reserved).
- Gamification — badges/points for visiting artifacts or completing trails (a `visits` or
  `achievements` table keyed on reader + artifact/trail; noted here as a placeholder since
  the specific mechanic wasn't scoped yet).
- Session cache (Redis) in front of reader token validation, only if request volume at 50k+
  active users actually shows Postgres round-trip latency as a bottleneck — not needed
  pre-emptively.

## 2.9 What stays constant across all of this

- One Postgres database, one object storage bucket, two Rust binaries, Caddy in front.
- `SKIP LOCKED` on Postgres as the job queue — no Redis needed for this system at this scale.
- Module-per-concern inside `pintrail-api`, each owning its own tables.
- Three distinct identity tiers (authors/readers/admins-as-author-role), never merged into
  one role enum.
