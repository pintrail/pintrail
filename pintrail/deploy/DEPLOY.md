# Deployment Guide

This document covers how to deploy Pintrail to a production Linux server using Docker Compose, Caddy, and systemd.

---

## Table of Contents

1. [Overview](#overview)
2. [Server Prerequisites](#server-prerequisites)
3. [Repository Layout on the Server](#repository-layout-on-the-server)
4. [Production Environment File](#production-environment-file)
5. [compose.prod.yml — Service Breakdown](#composeprodymml--service-breakdown)
6. [Caddyfile — Reverse Proxy](#caddyfile--reverse-proxy)
7. [Running Manually](#running-manually)
8. [Installing as a systemd Daemon](#installing-as-a-systemd-daemon)
9. [HTTPS & TLS Certificates](#https--tls-certificates)
10. [Named Volumes & Data Persistence](#named-volumes--data-persistence)
11. [Backups](#backups)
12. [Updating](#updating)
13. [User Management After First Boot](#user-management-after-first-boot)
14. [Startup Order & Health Checks](#startup-order--health-checks)
15. [Network Security](#network-security)

---

## Overview

Production runs six Docker containers managed by a single Compose file (`compose.prod.yml`):

```
internet
   │  (80, 443)
   ▼
 caddy            — TLS termination, gzip/zstd compression, reverse proxy
   │  (internal)
   ▼
 portal :8000     — FastAPI web application (UI + auth)
   │  (internal HTTP)
   ▼
 artifact :8001   — artifact data service (JSON REST API, owns artifact DB tables)
   │
   ├── postgres :5432   — primary data store (internal only)
   ├── redis    :6379   — job queue (internal only)
   └── worker          — background image processor (no port)
```

Only Caddy publishes ports to the host. All other services communicate over the Docker Compose internal network and are never reachable from outside.

---

## Server Prerequisites

- Linux server (Ubuntu/Debian recommended) with a public IP
- Domain name with an A record pointing to that IP
- Ports **80** and **443** open in the firewall
- **Docker Engine** installed (`docker.io` or the official Docker package)
- **Docker Compose plugin** installed (`docker compose` — not the legacy `docker-compose`)
- `systemd` (standard on all modern Linux distributions)

---

## Repository Layout on the Server

Clone the repository to `/opt/pintrail`:

```bash
git clone <repo-url> /opt/pintrail
```

The systemd service file (`deploy/pintrail.service`) hard-codes `/opt/pintrail` as its working directory. If you use a different path, update `WorkingDirectory` and `EnvironmentFile` in the service file before installing it.

---

## Production Environment File

Create `/opt/pintrail/.env` with the following variables. All values marked as required will cause `docker compose` to exit with an error if they are missing.

```env
# Domain — must match your DNS A record
PINTRAIL_DOMAIN=pintrail.example.com

# PostgreSQL credentials
POSTGRES_DB=pintrail
POSTGRES_USER=pintrail
POSTGRES_PASSWORD=replace-with-a-strong-db-password

# Initial admin account (seeded by the portal on first startup)
AUTH_ADMIN_EMAIL=admin@example.com
AUTH_ADMIN_PASSWORD=replace-with-a-strong-admin-password
AUTH_SESSION_TTL_HOURS=24

# Shared secret for internal portal → artifact and worker → artifact API calls
ARTIFACT_API_KEY=replace-with-a-strong-random-secret
```

**Never commit this file.** It is in `.gitignore`.

The portal seeds the admin user on first startup only — changing `AUTH_ADMIN_EMAIL` / `AUTH_ADMIN_PASSWORD` in `.env` after the account is already created has no effect. Use the admin UI or update the database directly.

---

## compose.prod.yml — Service Breakdown

### `caddy`

- Image: `caddy:2-alpine`
- Publishes ports **80** and **443** to the host
- Reads `./deploy/Caddyfile` (mounted read-only)
- Persists TLS certificates in the `caddy-data` volume
- Depends on `portal` being started (not healthy — Caddy tolerates a brief portal startup delay)
- `restart: unless-stopped` — restarts automatically on crash or reboot

### `postgres`

- Image: `postgres:16-alpine`
- No ports published to the host
- Data persisted in the `postgres-data` volume
- Health check: `pg_isready` polled every 5 seconds; both `portal` and `worker` wait for it before starting
- `restart: unless-stopped`

### `redis`

- Image: `redis:7-alpine`
- No ports published to the host
- Data persisted in the `redis-data` volume (ARQ job state)
- `restart: unless-stopped`

### `portal`

- Built from `./portal/Dockerfile`
- Runs on internal port 8000 (proxied by Caddy)
- Waits for `postgres` to pass its health check and `artifact` to pass its health check
- Mounts the `artifact-images` volume at `/data/images` (read: serves `/media/` files)
- `restart: unless-stopped`
- Sets `ENV=production` — disables Uvicorn auto-reload
- **No direct artifact DB access** — all artifact data goes through the artifact service

### `artifact`

- Built from `./artifact/Dockerfile`
- Runs on internal port 8001 (never exposed to the internet)
- Waits for `postgres` health check and `redis` to start
- Mounts the `artifact-images` volume at `/data/images` (read/write: owns image files)
- `restart: unless-stopped`
- Sole owner of `artifacts` and `artifact_images` database tables
- Requires `API_KEY` env var — must match `ARTIFACT_API_KEY` in portal and worker

### `worker`

- Built from `./worker/Dockerfile`
- No exposed ports
- Waits for `redis` to start and `artifact` to pass its health check
- Mounts the `artifact-images` volume at `/data/images` (writes processed WebP files)
- `restart: unless-stopped`
- **No direct database access** — reads image records and writes status via artifact service API

---

## Caddyfile — Reverse Proxy

`deploy/Caddyfile` is minimal:

```
{$PINTRAIL_DOMAIN} {
  encode zstd gzip

  reverse_proxy portal:8000
}
```

- `{$PINTRAIL_DOMAIN}` expands from the environment variable at Caddy startup.
- Caddy automatically provisions a Let's Encrypt TLS certificate for the domain.
- `encode zstd gzip` enables response compression.
- All traffic is forwarded to the `portal` container on port 8000 over the internal Docker network.

---

## Running Manually

From `/opt/pintrail`:

```bash
# Build images and start all containers in the background
docker compose -f compose.prod.yml up -d --build

# Check container status
docker compose -f compose.prod.yml ps

# Stream logs from all services
docker compose -f compose.prod.yml logs -f

# Stream logs from one service
docker compose -f compose.prod.yml logs -f portal

# Stop all containers (data volumes are preserved)
docker compose -f compose.prod.yml down
```

---

## Installing as a systemd Daemon

The service file at `deploy/pintrail.service` makes Docker Compose start automatically on boot and restart after failures.

```bash
# Copy the service file
sudo cp /opt/pintrail/deploy/pintrail.service /etc/systemd/system/pintrail.service

# Load the new file
sudo systemctl daemon-reload

# Enable (start on boot) and start now
sudo systemctl enable --now pintrail

# Check status
sudo systemctl status pintrail

# View logs
sudo journalctl -u pintrail -f
```

The service file uses `Type=oneshot` with `RemainAfterExit=yes`. This means systemd considers the service "active" as long as the containers are running, even though the `ExecStart` command returns immediately after `docker compose up -d` completes.

`ExecReload` is wired to rebuild and restart containers, so `sudo systemctl reload pintrail` (or `restart`) triggers a full image rebuild:

```bash
sudo systemctl restart pintrail
```

---

## HTTPS & TLS Certificates

Caddy handles TLS automatically via the ACME protocol (Let's Encrypt by default).

Requirements:
- The value of `PINTRAIL_DOMAIN` must resolve to the server's public IP before Caddy starts.
- Ports **80** and **443** must be reachable from the public internet (Caddy uses port 80 for the ACME HTTP challenge).

Certificates are stored in the `caddy-data` Docker volume and renewed automatically. No manual certificate management is needed.

---

## Named Volumes & Data Persistence

| Volume | Contents | Used by |
|---|---|---|
| `postgres-data` | All database tables and rows | postgres |
| `redis-data` | ARQ job queue state | redis |
| `artifact-images` | Original uploads + processed WebP files | portal, worker |
| `caddy-data` | TLS certificates and ACME account keys | caddy |
| `caddy-config` | Caddy runtime configuration | caddy |

`docker compose down` preserves all volumes. To destroy data:

```bash
docker compose -f compose.prod.yml down -v
```

---

## Backups

The three volumes that contain user data are:

1. **`postgres-data`** — all users, sessions, artifacts, and image metadata
2. **`artifact-images`** — all uploaded and processed image files
3. **`caddy-data`** — TLS certificates (can be reprovisioned, but backing up avoids rate-limit delays)

Example backup using `docker run` to access a volume:

```bash
# Dump the database
docker exec $(docker compose -f compose.prod.yml ps -q postgres) \
  pg_dump -U pintrail pintrail > backup_$(date +%Y%m%d).sql

# Archive image files
docker run --rm \
  -v pintrail_artifact-images:/data \
  -v $(pwd):/backup \
  alpine tar czf /backup/images_$(date +%Y%m%d).tar.gz -C /data .
```

---

## Updating

```bash
cd /opt/pintrail
git pull
sudo systemctl restart pintrail
```

`systemctl restart` triggers `ExecStop` (compose down) followed by `ExecStart` (compose up --build), which rebuilds the portal and worker images from the updated source.

**Database schema changes**: the portal's `create_all` on startup handles additive schema changes (new tables, new columns with defaults). Destructive changes (renamed columns, dropped tables) require a manual migration before restarting.

---

## User Management After First Boot

Sign in with the admin credentials from `.env`. From the admin panel in the portal UI you can create and manage additional users.

Roles available: `viewer` (read-only), `editor` (create/edit/delete artifacts and images), `admin` (all editor permissions plus user management).

---

## Startup Order & Health Checks

```
postgres  ──(healthy)──► artifact ──(healthy)──► portal ──(started)──► caddy
                                   └──(healthy)──► worker
redis     ──(started)──► artifact
          └─(started)──► worker
```

- `postgres` uses `condition: service_healthy` (pg_isready) before artifact starts.
- `artifact` uses a curl health check (`GET /health`); portal and worker wait for it with `condition: service_healthy`.
- `redis` uses `condition: service_started` because ARQ handles transient Redis unavailability gracefully.
- `caddy` depends on `portal: condition: service_started`.

---

## Network Security

- Only Caddy has host-bound ports (80, 443). All other containers are on the internal Docker Compose network.
- PostgreSQL (5432) and Redis (6379) are never accessible from the host or the public internet.
- The portal's `ENV=production` flag configures cookie `secure` behaviour appropriately for HTTPS.
- Media files are served through the portal's authenticated media router — they are not served as raw static files by Caddy.
