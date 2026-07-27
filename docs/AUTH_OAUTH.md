# Social Sign-In — Backend Contract (Milestone 0)

*The implementable specification for the mobile MVP's critical-path backend PR:
OAuth sign-in (Apple + Google) plus the small email/password author-token
fallback. Design rationale lives in [`MOBILE_MVP.md`](MOBILE_MVP.md) §4–5; this
document is the contract an implementer builds and tests against.*

Scope of the PR this specifies:

1. `POST /auth/oauth/{provider}` — verify a provider identity token, resolve it
   to a tier, mint a bearer session.
2. `POST /authors/token` — email/password → author bearer token (fallback for
   provisioned authors; mirrors `POST /readers/login`).
3. One schema migration.
4. No changes to any existing route, extractor, or the web studio.

---

## 1. Endpoint: `POST /auth/oauth/{provider}`

`provider ∈ {apple, google}`. Unauthenticated. JSON body:

```jsonc
{
  "id_token": "<provider JWT>",       // required
  "nonce": "<raw nonce>",             // required — the value the app generated
  "name": {                            // optional; Apple sends the user's name
    "given": "Tim", "family": "R."     // ONCE, outside the JWT, on first auth.
  }                                    // Used only if a new reader is created.
}
```

### Success — `200`

```jsonc
{
  "token": "<64-char hex bearer token>",
  "expires_at": "2026-10-20T00:00:00Z",
  "tier": "reader",                    // or "author"
  "role": null,                        // readers: null; authors: "viewer" | "editor" | "admin"
  "profile": {
    "id": "<uuid>",
    "email": "walker@umass.edu",
    "display_name": null,              // readers only; null → app shows the pseudonym
    "email_verified": true
  }
}
```

The token is a standard Pintrail bearer session: 256-bit CSPRNG, SHA-256 stored
in `reader_sessions` or `author_sessions`. **Everything downstream is
unchanged** — the existing extractors already accept these sessions, which is
what keeps this PR small at the edges.

### Failures

| Status | Body `error` | When |
|---|---|---|
| `400` | `"unknown provider …"` / `"id_token and nonce are required"` | Bad path segment or malformed body |
| `401` | `"sign-in could not be verified"` | **Any** verification or account failure: bad signature, wrong `aud`/`iss`, expired, nonce mismatch, replayed token, Google `email_verified=false`, suspended account. One opaque message; the specific reason goes to the server log only. |
| `429` | `"rate limit exceeded"` | Per-IP quota (§6) |
| `503` | `"sign-in is temporarily unavailable"` | Provider JWKS unreachable **and** no cached keys. Distinct from 401 so the app can show "try again" instead of "sign-in failed". |

The 401 opacity matches the login endpoints: distinct failure reasons would let
an attacker probe which check they are failing.

---

## 2. Token verification

Both providers issue RS256-signed JWTs. Verification is server-side and
non-negotiable — the app is never trusted to have checked anything.

### Required checks (all providers, in order)

1. **Header `alg` is exactly `RS256`.** Reject anything else before touching
   keys — this closes algorithm-confusion attacks (`none`, HS256-with-public-key).
2. **Signature** against the provider's JWKS key matching the header `kid` (§2.3).
3. **`iss`** matches the provider (below).
4. **`aud`** is one of *our* configured client IDs (below). A valid Google token
   minted for some other app must fail here.
5. **`exp`** not passed and **`iat`** not in the future, with **60 s leeway**
   for clock skew.
6. **Nonce binding** (§2.4).
7. Extract **`sub`** (stable subject) and **`email`**. For Google additionally
   require **`email_verified == true`**; reject otherwise.

### 2.1 Apple

- `iss`: `https://appleid.apple.com`
- JWKS: `https://appleid.apple.com/auth/keys`
- `aud`: the iOS **bundle ID** (`APPLE_BUNDLE_IDS`, §7)
- Email: always present in the id_token when the email scope was requested —
  either the real address or a `@privaterelay.appleid.com` relay. Both are
  provider-verified; treat identically.
- **Name is not in the JWT.** Apple returns it once, in the native response, on
  the first authorization only — hence the optional `name` field in the request
  body. Persist it at creation or lose it forever.

### 2.2 Google

- `iss`: `https://accounts.google.com` **or** `accounts.google.com` (Google
  emits both; accept both).
- JWKS: `https://www.googleapis.com/oauth2/v3/certs`
- `aud`: one of `GOOGLE_CLIENT_IDS` (§7) — a comma-separated list, because the
  iOS, Android, and Expo-web flows can each carry a different client ID.
- `email_verified` must be `true`.
- Optional hosted-domain restriction: if `GOOGLE_HOSTED_DOMAIN` is set (e.g.
  `umass.edu`), require the `hd` claim to match. **Off by default** — the open
  question in MOBILE_MVP §12; the config flag means deciding later costs nothing.

### 2.3 JWKS handling

- Cache keys by `kid` per provider, in memory.
- On a token whose `kid` is not cached: refresh the JWKS, **rate-limited to at
  most one refresh per provider per 5 minutes** — otherwise an attacker sending
  garbage `kid`s makes us hammer Apple/Google (and they will throttle us).
- Background staleness: treat cached keys older than 24 h as refreshable on
  next use. Keys are *never* proactively deleted on refresh failure — serving
  with a stale key beats a 503.
- Fetch uses `reqwest` with **rustls** (`default-features = false`,
  `rustls-tls`) — this is the API's first outbound HTTP client, and the images
  are deliberately OpenSSL-free. JWT verification via the `jsonwebtoken` crate.

### 2.4 Nonce

The app generates a fresh random nonce per attempt (`expo-crypto`), passes it
into the native flow, and sends the **raw** value in the request body. The
server recomputes what the provider should have embedded:

| Provider | id_token `nonce` claim contains | Server check |
|---|---|---|
| Apple (via `expo-apple-authentication`) | SHA-256 hex of the raw nonce (the library hashes before sending to Apple) | `claim == sha256_hex(body.nonce)` |
| Google (via `expo-auth-session`) | The raw nonce as provided | `claim == body.nonce` |

> **Verify at implementation time** against the exact client library versions —
> the hashing behavior is a library convention, not a protocol constant, and a
> mismatch here bricks sign-in for one provider while the other works.

**Replay hardening:** additionally keep an in-memory single-use set of consumed
token hashes (`sha256(id_token)`), expiring entries at the token's `exp`. A
second presentation of the same id_token → 401. Same per-replica caveat as the
rate limiter (documented there): with N replicas an attacker gets N chances —
acceptable at this scale because provider tokens expire in minutes, and the
nonce already binds the token to one app-initiated attempt.

---

## 3. Identity resolution

After a token verifies, resolve `(provider, sub, email, name?)` to an account.
**Order matters, and the first match wins:**

```
1. lower(email) matches an authors row WHERE is_active
       → mint author session; respond tier=author, role=<their role>.
         (First OAuth author sign-in also stores oauth_provider/subject on the
          authors row, so later matches are by stable subject, not just email.)

2. (provider, sub) matches readers.oauth_provider/oauth_subject
       → reject if NOT is_active (opaque 401); else mint reader session.

3. lower(email) matches a readers row (any is_active=true reader)
       → LINK: if that reader has no oauth identity yet, set
         oauth_provider/oauth_subject to this one.
         If they are already linked to a DIFFERENT provider, sign them in
         WITHOUT overwriting the link (email is provider-verified — the same
         trust that justified the original link).
       → mint reader session.

4. No match → CREATE a reader:
         email, email_verified = true, password_hash = NULL,
         oauth_provider/oauth_subject set, display_name = NULL.
       → mint reader session.
```

Notes an implementer needs:

- **The author check is what makes one sign-in serve both tiers.** Authors are
  provisioned by an admin; the provisioned email *is* the authorization. The
  client never asserts a tier.
- **A suspended author** (`is_active = false`) does **not** match rule 1 and
  falls through to reader resolution — losing the author role should not bar a
  person from being an explorer. A suspended *reader* is rejected outright.
- **Subject beats email** (rule 2 before 3) so a provider-side email change
  cannot detach or hijack an account once linked.
- **Race on first sign-in:** two concurrent requests for the same new identity
  can both reach rule 4. The unique index on `(oauth_provider, oauth_subject)`
  (§4) makes one INSERT lose; handle the unique violation by re-running
  resolution (the second pass hits rule 2). Same pattern for the email-unique
  index.
- **Apple relay addresses:** a person who hides their email behind Apple's relay
  and *also* has a password/Google account under their real address will end up
  with two reader accounts (the emails genuinely differ). Accepted MVP behavior
  — arguably what a relay user is asking for. Multi-identity linking is the
  upgrade path if it ever matters.
- **`display_name` stays NULL at creation** even when Apple sends a name — the
  column is unique, and auto-deriving from real names invites collisions and
  publishes a real name the user never chose to publish. The pseudonym mechanism
  already covers display; the Apple-provided name may be stored (non-unique
  profile fields) for future use but is not the public handle. *(Gap flagged:
  there is no `PATCH /readers/me` to set a display name post-creation yet —
  fine for MVP because pseudonyms render everywhere, but note it in the
  backlog.)*

---

## 4. Schema migration

```sql
-- Social-only readers have no password.
ALTER TABLE readers ALTER COLUMN password_hash DROP NOT NULL;

ALTER TABLE readers ADD COLUMN oauth_provider TEXT;
ALTER TABLE readers ADD COLUMN oauth_subject  TEXT;

-- One identity per account (MVP); both set or both null; and every account
-- must still have at least one credential.
CREATE UNIQUE INDEX readers_oauth_identity_key
    ON readers (oauth_provider, oauth_subject)
    WHERE oauth_provider IS NOT NULL;
ALTER TABLE readers ADD CONSTRAINT readers_oauth_paired CHECK (
    (oauth_provider IS NULL) = (oauth_subject IS NULL)
);
ALTER TABLE readers ADD CONSTRAINT readers_has_credential CHECK (
    password_hash IS NOT NULL OR oauth_provider IS NOT NULL
);

-- Authors keep their password (CLI provisioning, studio login) but gain the
-- same optional link so OAuth matches by stable subject after first sign-in.
ALTER TABLE authors ADD COLUMN oauth_provider TEXT;
ALTER TABLE authors ADD COLUMN oauth_subject  TEXT;
CREATE UNIQUE INDEX authors_oauth_identity_key
    ON authors (oauth_provider, oauth_subject)
    WHERE oauth_provider IS NOT NULL;
ALTER TABLE authors ADD CONSTRAINT authors_oauth_paired CHECK (
    (oauth_provider IS NULL) = (oauth_subject IS NULL)
);
```

Ripples through existing code (small, but they exist):

- `Reader.password_hash` becomes `Option<String>`. `POST /readers/login` on a
  NULL hash must **dummy-verify and fail opaquely** — the timing defense already
  exists; a social-only account must not be distinguishable from a wrong
  password.
- **Password reset on a social-only account works and is a feature:** completing
  the reset proves mailbox control and *sets* a password, making the account
  hybrid. No code change beyond the nullable type; the reset flow already
  writes `password_hash`.
- Reader `SELECT` lists gain the two new columns where the struct is loaded.

---

## 5. Sessions & TTLs

| Session | Table | TTL | Rationale |
|---|---|---|---|
| Reader bearer (OAuth or password) | `reader_sessions` | **90 days** (existing) | Explorers shouldn't be logged out between campus visits |
| Author bearer (OAuth or `/authors/token`) | `author_sessions` | **30 days** (new constant) | Mobile persistence vs. elevated privilege — longer than the studio's 12 h cookie, far shorter than a reader's 90 d. Tunable. |
| Author cookie (web studio) | `author_sessions` | 12 h (unchanged) | Untouched by this PR |

Both bearer kinds live in the tables the extractors already query, so
suspension-revokes-sessions, logout, and password-reset-revokes-sessions all
keep working with zero changes.

---

## 6. Rate limiting & abuse

- `POST /auth/oauth/{provider}`: per-IP, reuse `LOGIN_PER_IP` (30 / 15 min).
  The endpoint can *create accounts*, so it must not be free to hammer; there
  is no per-account key because the account isn't known until after
  verification.
- `POST /authors/token`: identical treatment to `POST /readers/login` — both
  `LOGIN_PER_ACCOUNT` and `LOGIN_PER_IP`, opaque 401s, dummy-verify on unknown
  email, counters reset on success.
- The JWKS refresh rate-limit (§2.3) is the third leg — without it the endpoint
  is a lever for making us DoS ourselves against Apple's key server.

---

## 7. Configuration

| Env | Required | Example / default |
|---|---|---|
| `APPLE_BUNDLE_IDS` | for `apple` | `edu.umass.pintrail` (comma list) |
| `GOOGLE_CLIENT_IDS` | for `google` | `…apps.googleusercontent.com` (comma list: iOS, Android, web) |
| `GOOGLE_HOSTED_DOMAIN` | no | unset (any Google account); `umass.edu` to restrict |
| `AUTHOR_TOKEN_TTL_DAYS` | no | `30` |

A provider whose config is absent returns `400 "provider not configured"` —
deploys that only want Google (or neither, in CI) stay valid. Startup logs which
providers are live.

New workspace dependencies: `jsonwebtoken`, `reqwest` (rustls only). Both must
keep the no-OpenSSL property of the runtime images.

---

## 8. `POST /authors/token` (fallback)

Email/password → author bearer, mirroring `POST /readers/login` exactly:

- Request `{ "email", "password" }`; success returns the §1 envelope with
  `tier: "author"`, `role`, 30-day expiry.
- Opaque 401 for unknown email (dummy-verify), wrong password, and suspended —
  indistinguishable, same as every other login.
- `POST /authors/token/logout` (bearer-authenticated) deletes the session row.
- Rate-limited per §6.

This is deliberately tiny: it exists so a CLI-provisioned author who doesn't
use Apple/Google — or any author before OAuth config exists in a deployment —
can still sign in on mobile.

---

## 9. Test plan (the PR's definition of done)

Adversarial first — each of these **must fail closed**:

| # | Case | Expect |
|---|---|---|
| A1 | Token signed by the wrong key (self-signed, correct claims) | 401 |
| A2 | `alg: none` / `alg: HS256` | 401 (before any key lookup) |
| A3 | Valid Google token, `aud` = someone else's client ID | 401 |
| A4 | Wrong `iss` | 401 |
| A5 | Expired token (beyond 60 s leeway) | 401 |
| A6 | Nonce mismatch (wrong raw nonce for the embedded claim) | 401 |
| A7 | Same id_token presented twice | second → 401 |
| A8 | Google token with `email_verified: false` | 401 |
| A9 | Suspended reader's identity | 401, opaque |
| A10 | 31 rapid attempts from one IP | 429 |
| A11 | All 401 bodies byte-identical across A1–A9 | equal |

Functional:

| # | Case | Expect |
|---|---|---|
| F1 | New email, valid token | reader created, `email_verified=true`, `password_hash` NULL, session works on `/readers/me` |
| F2 | Email matches a provisioned **active** author | `tier=author`, correct `role`; token works on an author route (e.g. artifact create for editor+) and on `/authors/me` |
| F3 | Suspended author's email | falls through → reader path (F1 or F4), **no** author session |
| F4 | Email matches an existing password reader | linked (provider/subject set), signed in as that reader |
| F5 | Repeat sign-in resolves by `(provider, subject)` even if the token's email changed | same reader as before |
| F6 | Reader linked to Google signs in with Apple + same email | signed in, Google link **not** overwritten |
| F7 | Concurrent first sign-ins, same identity | exactly one reader row; both requests succeed |
| F8 | Password login against a social-only account | opaque 401, timing-equalized |
| F9 | Password reset on a social-only account | sets a password; both methods work afterward |
| F10 | Provider not configured | 400 |
| F11 | JWKS unreachable, empty cache | 503; with warm cache → verification proceeds |
| F12 | `/authors/token` happy path + opaque failures + logout revocation | per §8 |

JWKS tests run against a **local mock JWKS server** (spin up an axum route in
the test serving a generated RSA key set) — never against live Apple/Google in
CI. One ignored-by-default integration test may hit the real providers for
manual verification.

---

## 10. Non-goals of this PR

- Account deletion (`DELETE /readers/me`) — required before App Store
  submission (Guideline 5.1.1), tracked separately.
- Refresh-token flows with the providers — we mint our own long-lived sessions;
  re-auth is a fresh OAuth round.
- OAuth for the **web studio** — it stays email/password + cookie.
- Multi-provider linking UI, `PATCH /readers/me` (display name), profile
  editing — backlog.
