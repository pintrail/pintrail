# slim, fast, no-nonsense
FROM python:3.13-slim AS base

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1

WORKDIR /app

# system deps for psycopg and runtime niceties
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential curl ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY services/artifact/deps/pyproject.toml /app/

# install in a venv to keep it tidy
RUN python -m venv /venv && . /venv/bin/activate && \
    pip install --upgrade pip && \
    pip install --no-cache-dir -e .

COPY services/artifact/app /app

# copy shared libs
COPY libs /app/libs

COPY services/db /app/db

EXPOSE 8000
ENV PATH="/venv/bin:$PATH"
ENV PYTHONPATH="/app:/app/libs"

CMD ["uvicorn", "artifact_module:app", "--host", "0.0.0.0", "--port", "8001"]
