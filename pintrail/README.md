# Pintrail

Pintrail is a secured artifact portal built with NestJS, PostgreSQL, Redis, a background image worker, and a browser-based admin/editor UI.

## Development

Start the local development stack:

```bash
docker compose up --build
```

The development stack uses:

- `portal` on port `3000`
- `postgres` on port `5432`
- `redis` on port `6379`
- `worker` for image normalization

## Production Deployment

This repository includes a production deployment setup that:

- serves HTTPS on `443`
- redirects traffic through Caddy
- runs the application stack as Docker containers
- keeps the stack running as a `systemd` daemon

### Files

- [compose.prod.yml](/Users/richards/Git/pintrail/pintrail/compose.prod.yml)
- [deploy/Caddyfile](/Users/richards/Git/pintrail/pintrail/deploy/Caddyfile)
- [deploy/pintrail.service](/Users/richards/Git/pintrail/pintrail/deploy/pintrail.service)

### Prerequisites

You need:

- a Linux server with a public IP
- a domain name pointing to that server
- ports `80` and `443` open in the firewall
- Docker Engine installed
- Docker Compose plugin installed
- `systemd` available

### Recommended Server Layout

Clone or copy the repository to:

```bash
/opt/pintrail
```

### Production Environment File

Create:

```bash
/opt/pintrail/.env
```

Example:

```env
PINTRAIL_DOMAIN=pintrail.example.com

POSTGRES_DB=pintrail
POSTGRES_USER=pintrail
POSTGRES_PASSWORD=replace-with-a-strong-db-password

AUTH_ADMIN_EMAIL=admin@example.com
AUTH_ADMIN_PASSWORD=replace-with-a-strong-admin-password
AUTH_SESSION_TTL_HOURS=24
```

Important:

- `PINTRAIL_DOMAIN` must match your DNS name
- replace all example passwords before going live
- `AUTH_ADMIN_EMAIL` and `AUTH_ADMIN_PASSWORD` seed the initial admin account

### Start Production Manually

From `/opt/pintrail`:

```bash
docker compose -f compose.prod.yml up -d --build
```

Check status:

```bash
docker compose -f compose.prod.yml ps
```

View logs:

```bash
docker compose -f compose.prod.yml logs -f
```

Stop the stack:

```bash
docker compose -f compose.prod.yml down
```

### Install As A Daemon

Copy the service file:

```bash
sudo cp /opt/pintrail/deploy/pintrail.service /etc/systemd/system/pintrail.service
```

Reload `systemd` and enable the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable pintrail
sudo systemctl start pintrail
```

Check status:

```bash
sudo systemctl status pintrail
```

Restart after updates:

```bash
sudo systemctl restart pintrail
```

View service logs:

```bash
sudo journalctl -u pintrail -f
```

### HTTPS on 443

Caddy handles HTTPS automatically using Let's Encrypt.

Requirements:

- the domain in `PINTRAIL_DOMAIN` must resolve to your server
- ports `80` and `443` must be reachable from the public internet

Caddy stores certificates in the persistent `caddy-data` volume.

### User Management

After first boot, sign in with the seeded admin account from `.env`.

You can then create additional users in three ways:

1. Admin UI in the portal
2. Admin-only auth API
3. CLI command

CLI example:

```bash
cd /opt/pintrail/portal
DB_HOST=127.0.0.1 \
DB_PORT=5432 \
DB_NAME=pintrail \
DB_USER=pintrail \
DB_PASSWORD=replace-with-a-strong-db-password \
npm run user:create -- --email editor@example.com --password strongpass123 --role editor
```

### Backups

Back up these Docker volumes:

- `postgres-data`
- `artifact-images`
- `caddy-data`

### Updating

To deploy a new version:

```bash
cd /opt/pintrail
git pull
sudo systemctl restart pintrail
```

### Notes

- The production stack does not expose PostgreSQL, Redis, or the Nest app directly to the public internet.
- Only Caddy publishes ports `80` and `443`.
- Media access and portal routes remain protected by the application authentication layer.
