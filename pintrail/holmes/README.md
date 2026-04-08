# Holmes

Holmes is a persistent investigation container that lives on the same Docker network as your system. Use it to inspect, test, and load-test your services without installing anything on your host machine.

Holmes is included in the starter repository at https://github.com/umass-cs-426/starter-project. Both students and TAs use it. During sprint demos, the TA will open a shell in Holmes and verify your system from there.

---

## Connecting to Holmes

### Option 1 — Terminal (always works)

After `docker compose up --build` completes:

```bash
docker compose exec holmes bash
```

You are now inside the container. Your repository root is mounted at `/workspace`, so all your k6 scripts, SQL files, and other local files are immediately available.

From inside Holmes, every service in your `compose.yml` is reachable by its service name:

```bash
curl http://order-service:3000/health | jq .
curl http://restaurant-service:3001/menu | jq .
redis-cli -h redis ping
psql postgres://user:pass@postgres:5432/mydb
```

You do not need to know port numbers on the host — the Docker network handles routing by name.

---

### Option 2 — VS Code Docker Extension

The Docker extension gives you a GUI for managing containers and a one-click way to open a shell in Holmes without leaving VS Code.

#### Required extensions

When you open the starter folder in VS Code it will prompt you to install the recommended extensions. Accept the prompt, or install them manually:

| Extension | ID | Purpose |
| --------- | -- | ------- |
| Docker | `ms-azuretools.vscode-docker` | Container management sidebar, attach shell |
| Dev Containers | `ms-vscode-remote.remote-containers` | Open a full VS Code window attached to Holmes |

#### Attaching a shell (Docker extension)

1. Start your system: `docker compose up --build`
2. Click the **Docker** icon in the VS Code Activity Bar (whale icon on the left)
3. Expand **Containers** → find the `holmes` container (it will show as running)
4. Right-click `holmes` → **Attach Shell**

A terminal opens inside the Holmes container. You are now at `/workspace` with all tools available. This is equivalent to `docker compose exec holmes bash` but stays inside VS Code.

#### Opening a full VS Code window inside Holmes (Dev Containers)

This attaches the entire VS Code editor — including its terminal, file explorer, and extensions — to the Holmes container. Files you open are inside the container; the terminal runs inside the container.

1. Start your system: `docker compose up --build`
2. Click the **Docker** icon in the Activity Bar
3. Expand **Containers** → find `holmes`
4. Right-click `holmes` → **Attach Visual Studio Code**

A new VS Code window opens. The window title shows `[Container] holmes`. Open `/workspace` as the folder to browse your repo from inside the container.

> **Note:** Extensions installed in this attached window run inside the container, not on your host. You may be prompted to install extensions inside the container the first time.

#### Viewing container logs in VS Code

In the Docker sidebar, right-click any container → **View Logs** to stream its output directly in a VS Code output panel. This is useful for watching your services and workers without switching to a terminal.

---

## Tool Reference

### HTTP

| Tool     | What it does                   | Example                                   |
| -------- | ------------------------------ | ----------------------------------------- |
| `curl`   | Make HTTP requests             | `curl -s http://my-service:3000/health`   |
| `wget`   | Download files, test endpoints | `wget -qO- http://my-service:3000/health` |
| `httpie` | Human-friendly HTTP client     | `http GET http://my-service:3000/health`  |

#### Useful curl patterns

```bash
# Pretty-print a JSON response
curl -s http://my-service:3000/health | jq .

# POST with a JSON body
curl -s -X POST http://my-service:3000/orders \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{"item": "burger", "quantity": 2}' | jq .

# Include response headers
curl -si http://my-service:3000/health

# Time a request
curl -o /dev/null -s -w "time_total: %{time_total}s\n" http://my-service:3000/health

# Follow redirects, fail on HTTP errors
curl -fsSL http://my-service:3000/resource
```

---

### JSON and YAML

| Tool | What it does          | Example                                 |
| ---- | --------------------- | --------------------------------------- |
| `jq` | Query and format JSON | `curl -s .../health \| jq '.db'`        |
| `yq` | Query and format YAML | `yq '.services' /workspace/compose.yml` |

#### Useful jq patterns

```bash
# Extract a single field
curl -s http://my-service:3000/health | jq '.status'

# Filter an array
curl -s http://my-service:3000/events | jq '.events[] | select(.available == true)'

# Watch a value update in real time
watch -n 2 'curl -s http://my-worker:3002/health | jq "{queue_depth, dlq_depth, last_job_at}"'
```

---

### Databases

| Tool        | What it does      | Example                                      |
| ----------- | ----------------- | -------------------------------------------- |
| `psql`      | PostgreSQL client | `psql postgres://user:pass@postgres:5432/db` |
| `redis-cli` | Redis client      | `redis-cli -h redis`                         |

#### Useful psql patterns

```bash
# Connect and run a query inline
psql postgres://user:pass@postgres:5432/mydb -c "SELECT COUNT(*) FROM orders;"

# Interactive session
psql postgres://user:pass@postgres:5432/mydb
# then: \dt         list tables
#       \d orders   describe a table
#       \q          quit
```

#### Useful redis-cli patterns

```bash
# Connect interactively
redis-cli -h redis

# Check queue depth (list length)
redis-cli -h redis LLEN my-queue
redis-cli -h redis LLEN my-queue:dlq

# Inspect the next item without removing it
redis-cli -h redis LINDEX my-queue 0

# Watch queue depth in real time
watch -n 1 'redis-cli -h redis LLEN my-queue'

# Pub/sub — subscribe to a channel
redis-cli -h redis SUBSCRIBE my-channel

# List all keys matching a pattern
redis-cli -h redis KEYS "event:*"

# Check a cached value
redis-cli -h redis GET "event:3fa85f64"
```

---

### Load Testing

| Tool | What it does          |
| ---- | --------------------- |
| `k6` | Scripted load testing |

All k6 scripts in your repo are accessible at `/workspace/k6/`.

```bash
# Run a sprint test script
k6 run /workspace/k6/sprint-1.js

# Override VUs and duration from the command line
k6 run --vus 50 --duration 60s /workspace/k6/sprint-1.js

# Write results to a JSON file for later review
k6 run --out json=/workspace/k6/results-sprint-1.json /workspace/k6/sprint-1.js
```

---

### File Browsing

| Tool           | What it does                                    | Example                      |
| -------------- | ----------------------------------------------- | ---------------------------- |
| `lsd`          | Modern `ls` with icons and colors               | `lsd -la /workspace`         |
| `bat`          | `cat` with syntax highlighting and line numbers | `bat /workspace/compose.yml` |
| `fd`           | Fast file finder                                | `fd schema.sql /workspace`   |
| `rg` (ripgrep) | Fast content search                             | `rg "RPUSH" /workspace`      |

```bash
# List files as a tree
lsd --tree /workspace/k6

# Search for a string across all files
rg "queue_depth" /workspace

# Find files by name
fd Dockerfile /workspace

# View a file with syntax highlighting
bat /workspace/holmes/Dockerfile
```

---

### Editors

| Tool   | What it does                      |
| ------ | --------------------------------- |
| `nvim` | Neovim with LazyVim configuration |

```bash
nvim /workspace/k6/sprint-2-cache.js
```

LazyVim installs its plugins on the first launch — this requires internet access from the container. Subsequent launches are instant.

---

### Network and Process Inspection

| Tool          | What it does                      | Example                                   |
| ------------- | --------------------------------- | ----------------------------------------- |
| `ping`        | Check connectivity to a service   | `ping order-service`                      |
| `nc` (netcat) | Test a TCP port                   | `nc -zv redis 6379`                       |
| `dig`         | DNS lookup                        | `dig order-service`                       |
| `lsof`        | List open files and sockets       | `lsof -i :3000`                           |
| `watch`       | Re-run a command every N seconds  | `watch -n 2 'curl -s .../health \| jq .'` |
| `tmux`        | Multiple terminals in one session | `tmux new -s investigate`                 |

---

### Runtime Environments

| Tool     | Version       |
| -------- | ------------- |
| Node.js  | LTS (latest)  |
| npm      | latest        |
| Python 3 | system latest |
| pip      | included      |

```bash
# Run a Node.js script
node /workspace/scripts/seed.js

# Run a Python script
python3 /workspace/scripts/seed.py

# Install a package temporarily (for the life of the container)
pip install httpx
npm install -g autocannon
```

---

## Common Investigation Workflows

### Verify all health endpoints

```bash
for svc in order-service restaurant-service dispatch-worker; do
  echo "=== $svc ==="
  curl -s http://$svc:3000/health | jq '{status, db, redis}'
  echo
done
```

### Watch a queue drain during a burst test

Open two terminal tabs. In one, run the k6 test. In the other:

```bash
docker compose exec holmes bash
watch -n 1 'curl -s http://dispatch-worker:3002/health | jq "{queue_depth, dlq_depth, last_job_at}"'
```

### Inject a poison pill manually

```bash
redis-cli -h redis RPUSH my-queue '{"this": "is intentionally malformed"}'
```

Then confirm the worker caught it without crashing:

```bash
curl -s http://dispatch-worker:3002/health | jq '{status, dlq_depth}'
```

### Inspect a Postgres table after a burst

```bash
psql postgres://user:pass@postgres:5432/mydb \
  -c "SELECT status, COUNT(*) FROM orders GROUP BY status;"
```

### Check that idempotency works

```bash
KEY=$(uuidgen)

# Send the same request twice with the same idempotency key
curl -s -X POST http://order-service:3000/orders \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $KEY" \
  -d '{"item": "burger", "quantity": 1}' | jq .

curl -s -X POST http://order-service:3000/orders \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: $KEY" \
  -d '{"item": "burger", "quantity": 1}' | jq .

# Both responses should be identical; the DB should contain exactly one record
psql postgres://user:pass@postgres:5432/mydb \
  -c "SELECT COUNT(*) FROM orders WHERE idempotency_key = '$KEY';"
```

---

## Shell Aliases (pre-configured)

| Alias               | Expands to                           |
| ------------------- | ------------------------------------ |
| `ll`                | `lsd -la`                            |
| `l`                 | `lsd -l`                             |
| `lt`                | `lsd --tree`                         |
| `cat`               | `bat --paging=never`                 |
| `healthcheck <url>` | `watch -n 2 "curl -s <url> \| jq ."` |

```bash
# Example: live-poll a health endpoint
healthcheck http://order-service:3000/health
```
