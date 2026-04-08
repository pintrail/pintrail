# ── wk2 image-processing worker ──────────────────────────────────────────────
FROM python:3.12-slim

COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /usr/local/bin/

WORKDIR /srv/wk2

COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-dev --no-install-project

COPY config.py main.py ./

# Storage directory — mount the same volume as the web service
RUN mkdir -p ./data/images

CMD ["uv", "run", "--no-dev", "arq", "main.WorkerSettings"]
