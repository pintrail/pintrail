# Pintrail

Pintrail is a secured artifact management portal. It lets authenticated users create a hierarchical tree of artifacts (with names, descriptions, and GPS coordinates), attach images to each artifact, and view processed image galleries. Access is role-gated: viewers read, editors write, admins manage users.

## Quick Start (Development)

```bash
docker compose up --build
```

The development stack starts on:

- **portal** → http://localhost:8000
- **postgres** → localhost:5432
- **redis** → localhost:6379

Default admin credentials (set in `compose.yml`):

- Email: `admin@example.com`
- Password: `change-me-now`

## Local Development (without Docker)

To run services locally, you'll need PostgreSQL and Redis running (the easiest way is `docker compose up postgres redis`), then set environment variables for the services.

### Portal

```bash
cd portal

# Set environment variables (see .env.example for all options)
export DB_HOST=localhost
export DB_PORT=5432
export DB_USER=pintrail
export DB_PASSWORD=pintrail
export DB_NAME=pintrail
export REDIS_HOST=localhost
export REDIS_PORT=6379
export AUTH_ADMIN_EMAIL=admin@example.com
export AUTH_ADMIN_PASSWORD=change-me-now

# Run with hot-reload
uv run uvicorn app.main:app --reload
```

### Worker

```bash
cd worker

# Set the same environment variables as portal
export DB_HOST=localhost
export DB_PORT=5432
# ... etc (see .env.example)

# Run the background job worker
uv run arq main.WorkerSettings
```

### Using direnv

Alternatively, create `.envrc` files in `portal/` and `worker/` directories to automatically load environment variables:

```bash
# portal/.envrc
export DB_HOST=localhost
export DB_PORT=5432
export DB_USER=pintrail
export DB_PASSWORD=pintrail
export DB_NAME=pintrail
export REDIS_HOST=localhost
export REDIS_PORT=6379
export AUTH_ADMIN_EMAIL=admin@example.com
export AUTH_ADMIN_PASSWORD=change-me-now
```

Then use `direnv allow` to enable the auto-loading.

## Services

| Service  | Directory    | Description                               |
| -------- | ------------ | ----------------------------------------- |
| portal   | `portal/`    | FastAPI web application and UI            |
| worker   | `worker/`    | Background image processor (ARQ + Pillow) |
| postgres | Docker image | Primary database                          |
| redis    | Docker image | Job queue broker                          |

## Architecture

See [`ARCH.md`](ARCH.md) for a full system description.

- [`portal/PORTAL.md`](portal/PORTAL.md) — portal service internals
- [`worker/WORKER.md`](worker/WORKER.md) — worker service internals
- [`deploy/DEPLOY.md`](deploy/DEPLOY.md) — production deployment guide

## Production

See [`deploy/DEPLOY.md`](deploy/DEPLOY.md) for full instructions. The short version:

```bash
# On the server at /opt/pintrail
cp deploy/pintrail.service /etc/systemd/system/pintrail.service
systemctl daemon-reload
systemctl enable --now pintrail
```

Production uses `compose.prod.yml`, which adds Caddy for HTTPS termination and removes host-bound ports from all internal services.
