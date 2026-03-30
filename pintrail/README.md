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

Default admin credentials (set in `compose.yml`, override via `.env`):

- Email: `admin@example.com`
- Password: `change-me-now`

## Services

| Service | Directory | Description |
|---|---|---|
| portal | `portal/` | FastAPI web application and UI |
| worker | `worker/` | Background image processor (ARQ + Pillow) |
| postgres | Docker image | Primary database |
| redis | Docker image | Job queue broker |

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
