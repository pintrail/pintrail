# ── pt2 web application ──────────────────────────────────────────────────────
FROM python:3.12-slim AS base

# Install uv
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /usr/local/bin/

WORKDIR /srv/app

# Install dependencies first (layer-cached unless pyproject.toml/uv.lock change)
COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-dev --no-install-project

# Copy application source
COPY app/ ./app/

# Create the image storage directory
RUN mkdir -p ./data/images

EXPOSE 8000

ENV ENV=production

CMD ["uv", "run", "--no-dev", "uvicorn", "app.main:app", \
     "--host", "0.0.0.0", "--port", "8000", "--no-access-log"]
