# Deploying PinTrail

How to set up the PinTrail backend on a server for the first time, redeploy it,
roll it back, and look after its database backups.

This guide covers the Rust backend in `backend/`, which is what runs at
`pintrail.cs.umass.edu`. The legacy Python system in `pintrail/` has been
replaced and is no longer deployed.

**In a hurry?**

```sh
cd ~/pintrail/backend
deploy/redeploy.sh                 # deploy the latest main
deploy/rollback.sh                 # undo the last deploy
```

---

## 1. What gets deployed

Everything runs with Docker Compose on one host:

| Service | What it is |
|---|---|
| `caddy` | Terminates TLS on ports 80/443. Proxies `/media/*` to object storage and everything else to the API. |
| `api` | `pintrail-api`: the HTTP API, the admin panel (`/admin`) and the Studio (`/studio`). |
| `worker` | `pintrail-worker`: processes uploaded media (thumbnails, HEIC and PDF conversion). |
| `postgres` | PostgreSQL 17. It is both the database and the worker's job queue. Its data lives in the `pgdata` volume. |
| `minio` | Object storage. On the production host this is the Versity S3 Gateway; the service keeps the name `minio`. Its data lives in the `miniodata` volume. |
| `minio-init` | A one-off container that creates the storage bucket and exits. |

The stack is assembled from several compose files:

- `compose.yml`: the base stack (on its own, it is the local development setup).
- `compose.prod.yml`: production changes. It adds Caddy and TLS, closes the database and storage ports to the outside, and takes all credentials from `.env`.
- `compose.versity.yml`: swaps MinIO for the Versity S3 Gateway and mounts the host's TLS certificates into Caddy.

Which files are used is set by `COMPOSE_FILE` in `backend/.env` (see §3).

## 2. Server prerequisites

- Linux with Docker Engine and the Docker Compose plugin (`docker compose version` works).
- The deploy user is in the `docker` group (`sudo usermod -aG docker <user>`, then log in again). Running docker through `sudo` does not work: sudo drops the environment variables the scripts pass to compose, so a deploy would run the wrong images.
- `git`, plus read access to `github.com/pintrail/pintrail` from the server. The scripts run `git fetch`.
- The TLS certificate and key at the paths `compose.versity.yml` mounts:
  - `/etc/ssl/certs/mapindstudy-fullchain.pem`
  - `/etc/ssl/private/mapindstudy-mapindstudy.key`
- DNS for `pintrail.cs.umass.edu` (and `mapindstudy.cs.umass.edu`, which redirects to it) pointing at the host.
- Ports 80 and 443 open to the internet. Nothing else needs to be exposed.
- Enough disk for the images, the volumes, and 10 database dumps in `backend/backups/`.

## 3. Configuration: `backend/.env`

All production settings live in `backend/.env` on the server. The file is gitignored and **must never be committed**: it holds the database password and the storage keys.

| Variable | Required | Notes |
|---|---|---|
| `COMPOSE_FILE` | yes | Which compose files make up the stack. For the production host: `compose.yml:compose.prod.yml:compose.versity.yml`. The deploy scripts refuse to run without it, so they can never deploy the development setup by accident. |
| `COMPOSE_PROJECT_NAME` | recommended | **Never change it once set.** Compose names the volumes after the project, so a new name means a new, empty database and storage. |
| `PINTRAIL_DOMAIN` | yes | `pintrail.cs.umass.edu`. Caddy serves this name. |
| `PUBLIC_BASE_URL` | yes | `https://pintrail.cs.umass.edu`. Used to build links in emails and as the allowed origin for browser uploads. |
| `POSTGRES_PASSWORD` | yes | A long random string. **Set it before the first start.** Postgres only reads it when it creates the database, so changing it later needs a manual `ALTER USER` as well. |
| `S3_BUCKET` | yes | Must be `media`. Caddy sends `/media/*` straight to storage without rewriting the path, so the path prefix *is* the bucket name. |
| `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY` | yes | Credentials for the bundled gateway. It is created with these, so pick random values. |
| `S3_ENDPOINT` | yes (bundled storage) | `http://minio:9000`, the gateway's address on the internal compose network. |
| `S3_PUBLIC_ENDPOINT` | yes (bundled storage) | `https://pintrail.cs.umass.edu`. This goes into the upload/download URLs handed to browsers and phones, which reach storage through Caddy. |
| `S3_REGION` | no | Default `us-east-1`. |
| `RUST_LOG` | no | Default `pintrail_api=info,pintrail_worker=info,warn`. |
| `MAILER` | no | Default `log`, which writes emails to the API log instead of sending them. A real mailer is still to be configured. |

Compose itself fails to start, naming the missing variable, if any of the required ones are unset.

## 4. First-time setup on a new server

1. **Clone the repository:**
   ```sh
   git clone https://github.com/pintrail/pintrail.git ~/pintrail
   cd ~/pintrail/backend
   ```
2. **Create `.env`** from the table in §3, and lock down its permissions:
   ```sh
   chmod 600 .env
   ```
3. **Deploy:**
   ```sh
   deploy/redeploy.sh
   ```
   On a fresh server there is no database yet, so the script prints `postgres is not running, so there is nothing to back up` and skips the backup. Everything else runs normally. The first build takes several minutes.
4. **Create the first admin account.** The password is read from the terminal and never appears in the shell history or process list:
   ```sh
   docker compose run --rm api create-author you@umass.edu admin
   ```
   Admins can create every other account from the browser, at **Admin → Authors → Add author**. The first time anyone opens the Studio, it asks for their name (and optionally a photo) before anything else.
5. **Check that it works:**
   ```sh
   curl https://pintrail.cs.umass.edu/health          # {"status":"ok",...}
   curl https://pintrail.cs.umass.edu/health/ready    # {"status":"ready"}
   ```
   Then sign in at `https://pintrail.cs.umass.edu/admin` and `/studio`.

### Moving an existing server onto the scripts

If the stack is already running from a manual `docker compose up`, you only need to:

1. `cd ~/pintrail/backend && git checkout main && git pull`
2. Add `COMPOSE_FILE=...` to `.env` if it isn't there.
3. Run `deploy/redeploy.sh`.

`deploy/rollback.sh` cannot go back to a commit from before the deploy scripts existed, because those builds lack the `pintrail-api migrate` command the scripts rely on. Until one scripted deploy has succeeded and a second one is on top of it, keep the manual procedure as your fallback: check out the old commit, then `docker compose up -d --build`.

## 5. Routine redeploy

```sh
cd ~/pintrail/backend
deploy/redeploy.sh                      # latest main from GitHub
deploy/redeploy.sh --ref v1.4.0         # a tag, branch, or commit hash
```

The script shows the commits and any new migrations about to go out, and asks you to confirm. Pass `--yes` to skip the question, for example when running from automation.

### What it does

| Step | Command it runs | If this step fails |
|---|---|---|
| 1. Check out | `git fetch`, then `git checkout --detach <commit>` | Nothing running was touched. |
| 2. Get images | `docker pull` the commit's images from GHCR (see *Images* below); builds on the host only if none were published | Nothing running was touched. |
| 3. Back up | `pg_dump` into `backups/` | Nothing running was touched. |
| 4. Migrate | `docker compose run --rm api migrate` | Nothing running was touched (see below). |
| 5. Replace containers | `docker compose up -d --remove-orphans` | The new version is (partly) running. Roll back. |
| 6. Health check | Waits up to 180s for `api` to be healthy and `worker` to be running | The new version is running but unhealthy. Roll back. |

The site stays up on the old version all through steps 1–4. So a broken build, a failed backup, or a failed migration never causes downtime.

**If a migration fails:** each migration runs in a transaction, so the failed one leaves no partial change. Earlier migrations in the same deploy may already have been applied, which is harmless if they were additive (§7). Fix the migration on a branch, merge it, and redeploy.

**After a failure in steps 1–4**, the git checkout is left at the new commit. The script prints the commit to go back to (`git checkout <old commit>`). This matters only if you want to run manual `docker compose` commands against the old code.

### Options

| Option | Meaning |
|---|---|
| `--ref <ref>` | What to deploy. Default `main`. A branch name deploys the GitHub version (`origin/<branch>`), not a possibly stale local copy. |
| `--yes`, `-y` | Don't ask for confirmation. |
| `--no-fetch` | Don't run `git fetch` first. |
| `--skip-backup` | Don't take a database backup. Only use this if you just took one another way. |
| `--build` | Build the images on the server instead of pulling them. For emergencies, such as GHCR being down. |

| Environment variable | Default | Meaning |
|---|---|---|
| `DOCKER` | `docker` | The docker command. Must not go through `sudo`; see the prerequisites. |
| `BACKUP_KEEP` | `10` | How many database dumps to keep. |
| `BACKUP_DIR` | `backups` | Where dumps and the deploy log go, relative to `backend/`. |
| `HEALTH_TIMEOUT` | `180` | Seconds to wait for the API to become healthy. |

### Rules the script enforces

- **The working tree must be clean.** A deploy must match a commit exactly. Never edit code on the server; commit, push, and deploy instead.
- **`COMPOSE_FILE` must be set** (§3).

### Images

CI builds the API and worker images for every commit on `main`, after the tests pass, and publishes them to GitHub's container registry:

- `ghcr.io/pintrail/pintrail-api:<commit>` and `:main`
- `ghcr.io/pintrail/pintrail-worker:<commit>` and `:main`

`redeploy.sh` sets `PINTRAIL_TAG` to the commit it deploys and pulls those images, so the server never compiles Rust and the image that runs is the one CI tested. Once the new version passes its health checks, the script tags the images `deployed`. `compose.prod.yml` uses that tag when `PINTRAIL_TAG` is unset, so `docker compose` commands you run by hand (`run --rm api create-author`, `restart`, `logs`) use the last successful deploy.

The server builds on the host instead, under the same names, when:

- `--build` is passed;
- the commit has no published images (it is not on `main`, or CI has not finished);
- the commit is from before CI published images, which matters when rolling back to one.

The images are built for `linux/amd64`.

**Registry access, once per server.** GHCR packages start private. Either make both packages public (GitHub → the `pintrail` organization → **Packages** → each package → **Package settings** → **Change visibility**), or log the deploy user in with a token that has `read:packages`:

```sh
docker login ghcr.io -u <github-user>     # paste the token as the password
```

The code is open source, so public images reveal nothing new. They contain no secrets; `.env` is excluded by `.dockerignore`.

### Automatic deploys

`deploy/auto-deploy.sh` deploys `main` without anyone logging in. A systemd timer runs it every five minutes. Each run:

1. Takes a lock, so two deploys never overlap.
2. Fetches `origin/main` and compares it with the last deployed commit in `backups/deploy.log`. If nothing changed, it exits quietly.
3. Asks GitHub whether the CI workflow (`.github/workflows/ci.yml`) passed on that commit. If CI is still running, it waits for the next run.
4. Runs `deploy/redeploy.sh --yes` for exactly that commit: pull its images, back up, migrate, replace the containers, check health. A green CI run on `main` means that commit's images are published.

It never rolls back on its own. When CI fails on main, or a deploy fails, it logs that, optionally opens a GitHub issue, and does not retry that commit (so a failed migration is not re-run every five minutes). The next commit on main is tried as usual.

A commit that was deployed before is never redeployed automatically. That keeps a rollback in place: after `deploy/rollback.sh`, `main` still points at the bad commit, and the timer leaves it alone until a new commit lands.

**Install it once on the server** (needs `sudo` for the unit files; the deploy itself runs as the deploy user):

1. Edit `User=` and the three paths in `deploy/systemd/pintrail-autodeploy.service` to match the host. Check with a dry run first, as the deploy user:
   ```sh
   cd ~/pintrail/backend
   deploy/auto-deploy.sh --dry-run
   ```
   It needs `git`, `curl`, `python3` and `flock` (util-linux), all standard on Ubuntu and Debian.
2. Optional but recommended: add a GitHub token to `backend/.env`. Without one the GitHub API allows 60 requests an hour per IP address, shared with everyone behind the same campus address. A fine-grained token for `pintrail/pintrail` with **Actions: read** is enough; add **Issues: write** and `AUTODEPLOY_ISSUES=1` to get an issue when something fails.
   ```sh
   AUTODEPLOY_GITHUB_TOKEN=github_pat_...
   AUTODEPLOY_ISSUES=1
   ```
3. Install and start the timer:
   ```sh
   sudo cp deploy/systemd/pintrail-autodeploy.{service,timer} /etc/systemd/system/
   sudo systemctl daemon-reload
   sudo systemctl enable --now pintrail-autodeploy.timer
   ```
   The copies in `/etc/systemd/system` do not update with the repository. Copy them again if the unit files change.

**Day to day:**

| Task | Command |
|---|---|
| When it next runs | `systemctl list-timers pintrail-autodeploy.timer` |
| What it did | `journalctl -u pintrail-autodeploy.service -n 100` |
| Run a check now | `sudo systemctl start pintrail-autodeploy.service` |
| Pause automatic deploys | `touch backups/auto-deploy.paused` (delete the file to resume) |
| Commits it gave up on | `cat backups/auto-deploy.state` |
| Retry a commit it gave up on | Deploy it by hand with `deploy/redeploy.sh --ref <commit>` |
| Turn it off | `sudo systemctl disable --now pintrail-autodeploy.timer` |

**Before merging a risky migration** (anything that is not purely additive, §7), pause automatic deploys, merge, and deploy by hand so someone is watching.

## 6. Rolling back

### Code only (the usual case)

```sh
deploy/rollback.sh
```

This redeploys whichever commit the last deploy replaced and leaves the database as it is. It goes through the same steps as a redeploy, including a fresh backup. It works because migrations are additive (§7), and because the API tolerates applied migrations it doesn't know about.

To roll back to a specific commit instead: `deploy/rollback.sh --to <commit>`.

Running `rollback.sh` twice undoes the rollback, because every deploy, rollbacks included, is appended to the deploy log.

### Code and database

```sh
deploy/rollback.sh --restore-db
```

Use this **only when a migration damaged data**, not merely because a migration ran. It:

1. Asks you to type `restore` to confirm (or pass `--yes`).
2. Takes a safety dump of the current database (`backups/…-pre-restore.dump`).
3. Stops `api` and `worker`.
4. Drops the database and restores it from the dump the last deploy took just before it migrated.
5. Redeploys the previous commit.

**Every change since that deploy is lost:** new accounts, artifacts, comments, uploads. Uploaded files remain in storage, but the database no longer knows about them.

To restore a different dump: `deploy/rollback.sh --backup backups/<file>.dump --to <commit>`. Always pair a dump with code from the same point in time.

If the restore itself fails, the script prints the command to put the safety dump back.

## 7. Writing migrations that can be rolled back

Migrations live in `backend/migrations/` as timestamped `.sql` files. They are built into the API binary and only ever run forward: there are no "down" migrations, and the database dumps are the undo mechanism.

To make code-only rollback work, **the previous release must still run against the new schema.** In practice:

- **Adding** a table, a nullable column, or a column with a default is safe.
- **Renaming or removing** a column takes two releases. First release: add the new column and have the code write both and read the new one. Next release: drop the old column.
- **Tightening a constraint** (`NOT NULL`, `CHECK`, unique) means first deploying code that already satisfies it and backfilling the data, then adding the constraint in a later release.
- **Never edit a migration that has already been deployed.** sqlx checksums applied migrations and refuses to start if one has changed. Write a new migration instead.

Every deploy lists the new migrations before asking you to confirm. That is the moment to check them.

## 8. Backups

- **Where:** `backend/backups/pintrail-<UTC time>-before-<commit>.dump`. Each deploy takes one, of the database *before* that deploy migrated it. The files are gitignored and readable only by their owner, because they contain password hashes and user content.
- **Retention:** the newest 10 are kept (`BACKUP_KEEP`). Safety dumps taken by `rollback.sh` are never deleted automatically.
- **Deploy log:** `backend/backups/deploy.log`, with one tab-separated line per deploy: time, previous commit, deployed commit, dump file.
- **Media files are not backed up** by these scripts. They live in the `miniodata` volume.

**Copy backups off the server.** A dump on the same disk as the database doesn't protect against losing the disk. For example, run this nightly from another machine:

```sh
rsync -a deploy-user@pintrail.cs.umass.edu:pintrail/backend/backups/ /safe/place/pintrail-backups/
```

Take a manual backup at any time:

```sh
cd ~/pintrail/backend
docker compose exec -T postgres pg_dump -U pintrail -d pintrail -Fc > backups/manual-$(date -u +%Y%m%dT%H%M%SZ).dump
```

Check that a dump is readable (this lists its contents and changes nothing):

```sh
docker compose exec -T postgres pg_restore --list < backups/<file>.dump | head
```

## 9. Day-to-day operations

Run all of these from `~/pintrail/backend`. Compose picks up `COMPOSE_FILE` from `.env`.

| Task | Command |
|---|---|
| Status of every service | `docker compose ps` |
| Follow the API logs | `docker compose logs -f api` |
| Follow the worker logs | `docker compose logs -f worker` |
| See outgoing emails (while `MAILER=log`) | `docker compose logs api \| grep -i mail` |
| Create an author | `docker compose run --rm api create-author <email> <viewer\|editor\|admin>` (or use the admin panel) |
| Reset a password (also signs the author out everywhere) | In the admin panel: **Authors → Edit → Reset password**, which also makes them choose their own on next sign-in. From the command line: `docker compose run --rm api reset-password <email>` (doesn't force a change; use it for your own account or when the panel is unavailable) |
| Change an author's email, name, or role; suspend or delete them | Admin panel: **Authors → Edit** |
| Which commit is deployed | From anywhere: `curl https://pintrail.cs.umass.edu/health` (its `commit` field) or **Admin → System**. On the server: `tail -1 backups/deploy.log` |
| What the search indexer is doing | **Admin → System**: progress, queue, failures with Retry, Re-index and Rebuild. JSON: `GET /api/admin/search/status` |
| Restart one service | `docker compose restart api` |
| Open a SQL shell | `docker compose exec postgres psql -U pintrail -d pintrail` |

Authors created from the command line keep the password you type. Authors created from the admin panel must choose a new password when they first sign in.

## 10. Troubleshooting

**`COMPOSE_FILE is not set`**: add it to `backend/.env` (§3).

**`the working tree has uncommitted changes`**: someone edited files on the server. See what changed with `git status` and `git diff`. Then either commit the change properly elsewhere, or discard it with `git checkout -- <file>`.

**`permission denied while trying to connect to the docker API`**: the user isn't in the `docker` group. Add them (`sudo usermod -aG docker <user>`) and log in again; for the timer, run `sudo systemctl restart pintrail-autodeploy.timer`.

**`api did not become healthy`**: the script prints the last 50 lines of the API log. Common causes:
- A missing or wrong `.env` value. The API exits at startup, naming the setting.
- The database is unreachable. Run `docker compose ps postgres` and `docker compose logs postgres`.
- A slow start on an overloaded host. Retry with `HEALTH_TIMEOUT=300`.

Then either fix the cause and redeploy, or run `deploy/rollback.sh`.

**`migration ... was previously applied but has been modified`**: a deployed migration file was edited. Revert the edit and put the change in a new migration (§7).

**The site is up but uploads fail or images don't load**: check `S3_BUCKET=media`, and that `S3_PUBLIC_ENDPOINT` is the public `https://` domain. Look at `docker compose logs caddy minio`.

**Certificate errors**: Caddy uses the certificate files mounted from `/etc/ssl/…` (§2). After the certificates are renewed on the host, run `docker compose restart caddy`.
