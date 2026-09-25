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
- The deploy user can run `docker`. Either add them to the `docker` group, or prefix every script with `DOCKER="sudo docker"`.
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
   Admins can create every other account from the browser, at **Admin → Authors → Add author**.
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
| 2. Build | `docker compose build --pull` | Nothing running was touched. |
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

| Environment variable | Default | Meaning |
|---|---|---|
| `DOCKER` | `docker` | Set `DOCKER="sudo docker"` if the deploy user isn't in the docker group. |
| `BACKUP_KEEP` | `10` | How many database dumps to keep. |
| `BACKUP_DIR` | `backups` | Where dumps and the deploy log go, relative to `backend/`. |
| `HEALTH_TIMEOUT` | `180` | Seconds to wait for the API to become healthy. |

### Rules the script enforces

- **The working tree must be clean.** A deploy must match a commit exactly. Never edit code on the server; commit, push, and deploy instead.
- **`COMPOSE_FILE` must be set** (§3).

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
| Reset a password (also signs the author out everywhere) | `docker compose run --rm api reset-password <email>` |
| Which commit is deployed | `git log --oneline -1` or `tail -1 backups/deploy.log` |
| Restart one service | `docker compose restart api` |
| Open a SQL shell | `docker compose exec postgres psql -U pintrail -d pintrail` |

Authors created from the command line keep the password you type. Authors created from the admin panel must choose a new password when they first sign in.

## 10. Troubleshooting

**`COMPOSE_FILE is not set`**: add it to `backend/.env` (§3).

**`the working tree has uncommitted changes`**: someone edited files on the server. See what changed with `git status` and `git diff`. Then either commit the change properly elsewhere, or discard it with `git checkout -- <file>`.

**`permission denied while trying to connect to the docker API`**: the user isn't in the `docker` group. Use `DOCKER="sudo docker" deploy/redeploy.sh`, or add the user to the group and log in again.

**`api did not become healthy`**: the script prints the last 50 lines of the API log. Common causes:
- A missing or wrong `.env` value. The API exits at startup, naming the setting.
- The database is unreachable. Run `docker compose ps postgres` and `docker compose logs postgres`.
- A slow start on an overloaded host. Retry with `HEALTH_TIMEOUT=300`.

Then either fix the cause and redeploy, or run `deploy/rollback.sh`.

**`migration ... was previously applied but has been modified`**: a deployed migration file was edited. Revert the edit and put the change in a new migration (§7).

**The site is up but uploads fail or images don't load**: check `S3_BUCKET=media`, and that `S3_PUBLIC_ENDPOINT` is the public `https://` domain. Look at `docker compose logs caddy minio`.

**Certificate errors**: Caddy uses the certificate files mounted from `/etc/ssl/…` (§2). After the certificates are renewed on the host, run `docker compose restart caddy`.
