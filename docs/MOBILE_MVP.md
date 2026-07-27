# Pintrail Mobile — MVP Design

*The React Native (Expo) app. This document is the plan of record for the first
shippable version; it complements the backend spec in [`DESIGN.md`](DESIGN.md)
and does not restate it.*

---

## 1. Scope

**One role-aware app** serving both kinds of user, with the UI adapting to who
is signed in:

- **Explorers** (the public, reader tier): sign in, browse artifacts on a map
  and a "near me" list, open an artifact to read it and see its photos, comment,
  and build + share their own trails.
- **Authors** (editor/admin tier): everything above, plus create and edit
  artifacts (with photos and a map-placed location) and curate trails.

Crucially, **there is no "pick your role" step**. Everyone signs in the same
way (§5); the server decides the tier from the verified identity, and the app
reveals authoring tools only when the account's role is `editor` or `admin`. An
explorer never sees them, and the decision is server-truth, not a client toggle.

### In scope for the MVP

- **Social sign-in** — Continue with Apple / Continue with Google. Explorer
  accounts are created on first sign-in; author accounts are recognized
  automatically. Email/password remains as a fallback (§5).
- Nearby: a map with artifact pins + a distance-sorted list, using the device's
  **foreground** location and the `/artifacts/sync` manifest.
- Artifact detail: description, photo gallery, comments (read + post).
- Personal trails: list mine, view one, build/reorder stops, share by link,
  open a shared link.
- Authoring (editor+): create/edit an artifact — name, kind, description,
  map-placed coordinates (blank = inherit from parent), and photo upload;
  create/curate a trail.

### Explicitly *not* in the MVP (fast-follow, in priority order)

1. **Background geofencing + notifications** ("You're near Elm Building"). The
   product's signature feature, but the heaviest RN piece — background-location
   permission, `expo-task-manager`, per-platform region limits, battery tuning.
   The MVP's foreground "near me" list is the stepping stone; the data path
   (`/artifacts/sync`) is already the right one for it.
2. Audio/video attachments (backend worker doesn't process them yet either).
3. BLE beacon room-level precision (`beacon_id` is reserved but unused).
4. Offline media, push notifications, gamification.

---

## 2. Decisions locked

| Decision | Choice | Consequence |
|---|---|---|
| App structure | One role-aware app | One codebase, one store listing; authoring UI gated on role |
| Sign-in | **Social first — Apple + Google**, email/password as fallback | Lowest join friction; unifies explorer + author entry into one screen; new backend OAuth endpoint (§4) |
| Auth transport | Bearer tokens for both tiers on mobile | Web studio keeps its cookie flow; mobile is bearer, however the token is obtained |
| Location in MVP | Foreground map + nearby list | No background-location permission; background geofencing is the first fast-follow |
| Tooling | Existing Expo scaffold | Expo SDK 54, expo-router, TypeScript, `react-native-maps` already present |

**App Store rule that forces the pair:** Apple requires *Sign in with Apple* on
iOS if the app offers any other third-party social login (Google). So Apple is
not optional once Google is in — both ship, and the two-button screen is exactly
what Apple expects anyway.

---

## 3. Tech stack

The `mobile/` scaffold already fixes most of this. Additions the MVP needs:

| Concern | Choice | Notes |
|---|---|---|
| Framework | Expo (SDK 54) + **development build** | Native modules (maps, camera, location, Apple auth) — Expo Go is not enough; use `expo-dev-client` + EAS |
| Routing | `expo-router` (present) | File-based; structure in §6 |
| Server state | **TanStack Query** | Caching, retries, and the sync-manifest refresh fit its model cleanly |
| Apple sign-in | **`expo-apple-authentication`** | First-party Apple button; returns an identity token (JWT) we verify server-side |
| Google sign-in | **`expo-auth-session`** (Google provider) or `@react-native-google-signin` | Returns an ID token we verify server-side |
| Nonce for OAuth | **`expo-crypto`** | Generate a per-attempt nonce, bound into the token to block replay (§4) |
| Auth token storage | **`expo-secure-store`** | Bearer tokens in the OS keychain/keystore, never AsyncStorage |
| Location | **`expo-location`** (foreground only for MVP) | `getCurrentPositionAsync` + `watchPositionAsync`; background APIs deferred |
| Maps | `react-native-maps` (present) | Apple Maps on iOS / Google on Android; artifact pins + user location |
| Photos | **`expo-image-picker`** (+ `expo-camera` if capturing in-app) | Returns a local `file://` URI and a MIME type; feeds the presigned upload (§8) |
| Local cache | **`expo-sqlite`** (or MMKV) | Persist the sync manifest so the map loads instantly and to prep for background geofencing |

---

## 4. Required backend change: social sign-in

This is the largest backend addition the MVP needs, and it is the one thing that
should ship (and be tested) before app work. The transport stays bearer tokens
as already decided — OAuth is simply *how the bearer token is obtained*.

### 4.1 New endpoint

`POST /auth/oauth/{provider}` where `provider ∈ {apple, google}`. Body carries
the provider's **identity token** (a JWT `id_token`) and the **nonce** the app
generated for this attempt. The backend:

1. **Verifies the token** against the provider's published keys (JWKS): checks
   the signature, `iss` (Apple/Google), `aud` (our client ID), `exp`, and that
   the embedded nonce matches the one the app sent. A token that fails any of
   these is rejected — the app is not trusted to have done this.
2. **Extracts** the stable subject `sub` and the (provider-verified) `email`.
3. **Resolves the identity → tier** (order matters, see 4.3), mints a bearer
   session in the existing `reader_sessions` or `author_sessions` table, and
   returns `{ token, expires_at, tier, role, profile }`.

### 4.2 Schema additions (migration)

- On `readers`: add `oauth_provider TEXT` and `oauth_subject TEXT`, with a
  unique index on `(oauth_provider, oauth_subject)`. **Make `password_hash`
  nullable** — an OAuth-only reader has no password.
- Optionally the same columns on `authors`, populated on first OAuth link so the
  account is thereafter matched by stable subject, not just email.

### 4.3 Identity resolution (the rule that removes the toggle)

On a verified OAuth identity, in order:

1. If the email matches an **active author** → author session. *(Authors are
   provisioned by an admin; matching a provisioned email is what grants the
   author role — the client never asserts it.)*
2. Else if `(provider, subject)` matches an existing **reader** → sign that
   reader in.
3. Else if the email matches an existing reader → **link** this provider to it.
4. Else **create** a new reader.

Linking prefers the stable `subject` over email, so a later email reassignment
by the provider cannot silently hijack an account; the first link is by verified
email, which is the strongest signal available at account creation.

### 4.4 Verification gate mostly disappears

A provider-verified email means the reader is created with `email_verified =
true`, so the `VerifiedReader` gate (comments, trails) passes immediately for
social sign-in. Email verification and the SMTP mailer therefore back only the
**email/password fallback** path and future notifications — not the primary
flow.

### 4.5 Email/password fallback stays

The existing `POST /readers/login` (bearer) and `POST /authors/token` (a small
new bearer login mirroring readers, for CLI-provisioned authors who don't use
OAuth) remain, so nobody without Apple/Google is locked out and the web studio's
cookie flow is untouched. The app leads with the social buttons and tucks
email/password behind a secondary link.

> **Effort note:** this is meaningfully more than the plain "author bearer
> token" idea from the first draft — token verification against provider JWKS,
> the nonce dance, account linking, and a schema migration. It is the right
> investment for "easy to join + professional," but it is the critical-path
> backend work and should be scoped as its own PR with its own tests
> (forged token rejected, nonce replay rejected, author-email → author role,
> new email → reader, provider relink by subject).

---

## 5. Auth model in the app

**One sign-in screen, the same for everyone.** Two big buttons — Continue with
Apple, Continue with Google — and a quiet "Use email instead" link for the
fallback. No "explorer or author?" choice anywhere.

Flow:

1. App generates a nonce (`expo-crypto`), runs the native Apple/Google flow, and
   receives an identity token.
2. App `POST`s it to `/auth/oauth/{provider}` → `{ token, tier, role }`.
3. Token goes to `expo-secure-store`; an `AuthContext` holds `{ token, tier,
   role }`. Every API call attaches `Authorization: Bearer <token>`.
4. The UI renders from `role`: `editor`/`admin` unlock the authoring entry
   points; everyone else sees the explorer experience only. The `author/` routes
   also guard on role so a deep link can't bypass the UI gate.

Error handling: a `401` clears the token and returns to sign-in. For the
email/password fallback only, a `403 EmailNotVerified` routes to a "confirm your
email" screen with a **resend** action (the backend returns that as a distinct
error precisely so the client can tell it apart from a real auth failure).

**Why this is the good design, not just the easy one:** the tier is never a
client claim — an attacker cannot request "author" by flipping a toggle, because
the role is derived server-side from a provider-verified email checked against
the provisioned-authors table. The app is purely a renderer of whatever the
server says the account is.

---

## 6. Navigation / screen map (expo-router)

```
app/
  _layout.tsx              # AuthProvider + QueryClient + theme; splash until token resolved
  index.tsx                # redirect: -> (tabs) if signed in, else (auth)/sign-in

  (auth)/
    sign-in.tsx            # the hero: Continue with Apple / Google + "use email instead"
    email.tsx              # fallback: email/password sign-in + explorer register
    verify.tsx             # only reached from the email fallback: "check your email" + resend

  (tabs)/
    _layout.tsx            # tab bar: Nearby · Trails · Me
    nearby.tsx             # map + distance-sorted list (§7)
    trails.tsx             # my trails, public trails, "open a shared link"
    me.tsx                 # profile, role, sign out

  artifact/[id].tsx        # detail: gallery, description, comments
  trail/[id].tsx           # trail view: ordered stops, each links to its artifact
  trail/new.tsx            # build/edit a personal trail (add + drag-reorder stops)

  author/                  # rendered/reachable only when role >= editor
    artifact-new.tsx       # create: name, kind, description, map-pick location, photos
    artifact-edit-[id].tsx # edit the same
```

The social path skips `register.tsx` entirely — a reader account is created on
first sign-in. `email.tsx`/`verify.tsx` exist only for the fallback.

Authoring entry points (a "+" on Nearby, an "Edit" on artifact detail) are
**conditionally rendered on role**.

### 6.1 Sign-in screen — visual specification

The first thing anyone sees. It carries no data and asks no questions, so all of
its job is *impression*: a real place, a clear mark, two obvious ways in. Spec is
buildable as written; numbers are in points (pt).

#### Composition (portrait)

```
┌───────────────────────────────┐  ← full-bleed hero image, edge to edge,
│                                │    behind the status bar (no top inset)
│                                │
│         [ ◈ pin mark ]         │  ← brand block, optically centered in the
│          P I N T R A I L       │    upper third
│      Discover the campus.      │
│                                │
│                                │
│              ·                 │  ← flexible spacer (hero breathes here)
│                                │
│   ┌─────────────────────────┐  │
│   │    Continue with Apple   │  │  ← auth stack, pinned to the lower third
│   └─────────────────────────┘  │
│   ┌─────────────────────────┐  │
│   │  [G]  Continue with Google│ │
│   └─────────────────────────┘  │
│                                │
│        Use email instead        │  ← quiet text link
│                                │
│   By continuing you agree to    │  ← legal microcopy, 2 lines max
│   the Terms and Privacy Policy. │
└───────────────────────────────┘  ← buttons sit above the bottom safe-area inset
```

A dark **gradient scrim** sits between the hero and the content
(`rgba(0,0,0,0)` at ~40% height → `rgba(0,0,0,0.65)` at the bottom) so text and
buttons stay legible over any photo. The brand block gets its own lighter top
scrim if the chosen image is bright at the top.

#### Anatomy & spacing (top → bottom)

| Element | Spec |
|---|---|
| Hero image | Full-bleed, `cover`, extends under the status bar. Ships light- and dark-toned variants (or one image the scrim tames for both). |
| Pin mark | App logo mark, ~64pt, centered. |
| Wordmark | "Pintrail", 34pt / weight 700, letter-spacing +0.5, white. |
| Tagline | One line, 16pt / weight 400, white at 85% opacity. 20pt below wordmark. |
| Spacer | Flexible — pushes brand up, auth stack down; both anchor to thirds, not the exact center. |
| Auth buttons | Two, full width minus **24pt** side margins, **52pt** tall, **12pt** corner radius, **12pt** gap between them. |
| "Use email instead" | 15pt / weight 500, white, centered, 20pt below the buttons, 44pt tap target. |
| Legal line | 12pt / weight 400, white at 60%, centered; "Terms" and "Privacy Policy" tappable. 16pt above the bottom safe inset. |

#### Buttons (platform-compliant, equal prominence)

Apple **requires** its button be no less prominent than any other sign-in
option, so both buttons share identical width, height (52pt), and radius (12pt).

- **Apple** — `expo-apple-authentication`'s `AppleAuthenticationButton`
  (`SIGN_IN`/`CONTINUE`), style `BLACK` in light mode, `WHITE` in dark, corner
  radius 12. Do not re-implement it; the native component keeps Apple's label,
  logo, and localization correct.
- **Google** — a custom button that follows Google's current Identity branding
  exactly: white background `#FFFFFF`, text `#1F1F1F` weight 500, the **unmodified**
  colored "G" mark from Google's asset kit at 20pt, left-aligned logo with the
  label optically centered. Dark variant: background `#131314`, text `#E3E3E3`.
  Never recolor or redraw the G.
- **Order:** Apple first on iOS (platform convention + prominence); on Android,
  Google first. Drive off `Platform.OS`.

#### States

| State | Behavior |
|---|---|
| Idle | Both buttons enabled. |
| Pressed | Native press feedback (Apple's own; Google button uses `Pressable` with an 8% overlay). |
| In progress | The tapped button shows an inline spinner and its label reads "Signing in…"; the other button and the email link dim to 40% and disable, so only one flow runs at a time. |
| Cancelled | User backed out of the provider sheet → silently restore idle (no error; cancellation is not failure). |
| Error | A dismissible banner slides in above the auth stack ("Couldn't sign in — try again"); buttons return to idle. Network vs. provider errors get the same friendly copy; details go to logs, not the screen. |

#### Motion (all gated on Reduce Motion)

- Entrance: brand block and auth stack fade + rise 12pt, staggered ~80ms, over
  ~400ms on first mount.
- Hero: a slow, subtle Ken Burns drift (scale 1.0 → 1.06 over ~20s, alternating).
  Off entirely when Reduce Motion is on — it becomes a static image.
- Nothing bounces or spins decoratively; motion is a settle, not a show.

#### Accessibility

- Scrim guarantees ≥ 4.5:1 contrast for all text and the "use email" link over
  the hero.
- Every control ≥ 44pt tap target (buttons are 52).
- VoiceOver labels: "Continue with Apple", "Continue with Google", "Use email
  instead", and the two legal links; the pin mark is decorative (hidden).
- Supports Dynamic Type up to the large accessibility sizes — the wordmark and
  tagline scale, and the auth stack stays pinned above the safe inset (it
  scrolls if type is enormous rather than colliding with the buttons).
- Full light/dark support via the button variants and the two hero tones.

#### Assets to produce

- Hero image(s): a strong campus photograph (or two, for light/dark), 1284×2778
  @3x, licensed for the app.
- App pin/logo mark as SVG (also the source of the tab-bar and launcher icons).
- Google "G" from Google's official kit (do not trace your own).
- Wordmark typeface decision — a single display weight is enough for MVP;
  system font is an acceptable start if a brand face isn't chosen yet.

#### Copy

- Wordmark: **Pintrail**
- Tagline (pick one): "Discover the campus." / "Find what's around you." /
  "Every place has a story."
- Buttons: "Continue with Apple", "Continue with Google"
- Fallback: "Use email instead"
- Legal: "By continuing you agree to the Terms and Privacy Policy."

Palette note: the accent (`#2563eb` light / `#3b82f6` dark) matches the web
studio, so the two Pintrail surfaces read as one product.

---

## 7. Location & "nearby" (MVP, foreground)

The location-triggered premise, without background permissions yet:

1. On first open of Nearby, request **foreground** location permission.
2. Pull `GET /artifacts/sync?since=<cursor>` and cache the manifest locally
   (id, kind, name, parent, **resolved** lat/lng, `sync_version`, tombstone).
   The cursor persists so subsequent syncs are incremental; tombstones evict.
3. With the device position, compute distance to each cached artifact's
   effective coordinates **client-side** (no server round trip per move) and
   render: a map centered on the user with pins, and a list sorted by distance.
4. Tapping a pin or row opens `artifact/[id]`.

This reuses the exact data path background geofencing will need, so the
fast-follow is additive: register the cached coordinates with the OS region
monitor and raise a notification instead of (only) drawing a list.

---

## 8. Media capture & upload

Authoring photos ride the existing three-step presigned flow — the same one the
web studio drives, and the same endpoints:

1. `POST /artifacts/{id}/attachments/upload-intent` → `{ upload_url,
   attachment_id, required_headers }`.
2. **PUT the bytes** to `upload_url` with the exact `content-type` from
   `required_headers` (it is signed into the URL).
3. `POST /attachments/{id}/complete`; then poll `GET /artifacts/{id}` until the
   attachment's status is `processed` and its thumbnail URL appears.

Notes specific to RN:

- `expo-image-picker` returns a `file://` URI + MIME. For step 2, read it into a
  blob and PUT that; don't wrap it in `FormData` — the presigned PUT wants the
  raw body.
- **iPhone photos are HEIC.** The backend already accepts `image/heic` and the
  worker decodes it, so no client-side conversion is needed — send the original.
- The upload is author-only, so it lives behind the role gate.

---

## 9. The local-development gotcha (call this out early)

A phone is **not** `localhost`. Two things bite, and one is a repeat of a bug
already fought in the Docker work:

1. **API base URL.** A physical device (and often the simulator) cannot reach
   `http://localhost:8080`. Use the dev machine's LAN IP (`http://192.168.x.y:8080`)
   or an Expo tunnel. Make the API base URL an env/config value, not a constant.
2. **Presigned media URLs must be device-reachable.** Same two-endpoint issue
   from the container work: the API signs download/upload URLs against
   `S3_PUBLIC_ENDPOINT`, and that host must resolve **from the phone**.
   `localhost:9000` won't; set `S3_PUBLIC_ENDPOINT` to the LAN IP for device
   testing. If this is wrong, everything works *except* images silently fail to
   load — exactly the failure mode that stalled uploads in compose.

There's also an **OAuth redirect/config** dimension: Apple and Google both need
the app's bundle/client IDs registered, and Google needs the correct redirect
scheme. These belong in the dev setup doc, not per-person discovery.

---

## 10. Screen → API map

| Screen | Endpoints |
|---|---|
| Sign-in (social) | `POST /auth/oauth/apple`, `POST /auth/oauth/google` *(new, §4)* |
| Sign-in (email fallback) | `POST /readers/login`; `POST /authors/token` *(new)*; `POST /readers/register`, `/readers/verify-email`, `/readers/resend-verification` |
| Me | `GET /readers/me` or `GET /authors/me` (role) |
| Nearby | `GET /artifacts/sync?since=` |
| Artifact detail | `GET /artifacts/{id}` (incl. attachments), `GET/POST /artifacts/{id}/comments`, `DELETE /comments/{id}` |
| Trails list | `GET /trails?owner=me`, `GET /trails` (public), `GET /trails/shared/{token}` |
| Trail view / build | `GET /trails/{id}`, `POST /trails`, `PATCH /trails/{id}`, `PUT /trails/{id}/stops`, `DELETE /trails/{id}` |
| Create/edit artifact (author) | `POST /artifacts`, `PATCH /artifacts/{id}`, `POST /artifacts/{id}/attachments/upload-intent`, `POST /attachments/{id}/complete` |

New surface: the two `/auth/oauth/*` endpoints and the small `/authors/token`
fallback. Everything else exists.

---

## 11. Build order (milestones)

0. **Backend — social sign-in (§4):** the OAuth endpoint (Apple + Google token
   verification, nonce, identity resolution), the schema migration, and the
   `/authors/token` fallback. Its own PR, with the adversarial tests listed in
   §4.5. Ships first — it is the critical path.
1. **App shell + auth:** the sign-in screen (Apple/Google + email fallback),
   nonce + native flows, secure token storage, `AuthContext`, role-driven
   rendering, the API client with bearer + 401/403 handling.
2. **Explore:** Nearby (map + list from the sync manifest, foreground location),
   artifact detail with gallery + comments.
3. **Personal trails:** list, view, build/reorder, share-link resolve.
4. **Authoring (editor+):** create/edit artifact with map-pick location and
   photo upload; curated trail create.
5. **Polish:** offline manifest cache, pull-to-refresh, empty/error states, the
   dev-config from §9, and the sign-in screen's visual finish.

**Fast-follow after MVP:** background geofencing + local notifications (§1),
then audio/video, then BLE.

---

## 12. Open questions for later

- **Google client type:** UMass Google Workspace accounts make Google sign-in
  natural for the campus; confirm whether to restrict Google to the `umass.edu`
  hosted domain for explorers or allow any Google account.
- **Apple private-relay emails:** Apple can hand back a relay address rather than
  the real one, and only returns the user's name on the *first* authorization —
  the backend must persist the name/email on first link and not expect them
  again.
- **Author onboarding via OAuth:** `create-author` currently requires a password
  (for the studio cookie login). For OAuth-only authors it could gain an
  email-only mode; decide whether provisioned authors always also get a studio
  password.
- Push notifications (Expo Push) — needed only once background geofencing lands.
- App store presence: a single listing means the authoring UI ships in the
  public binary (gated, never shown to explorers). Fine for a campus tool.
