# Local candidate for pgvector (#25) plus future PostGIS work (#100).
# Do not attach this image to an existing Alpine data directory. Test a logical
# backup/restore into a separate volume before agreeing a production rollout.
FROM pgvector/pgvector:0.8.7-pg17-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        postgresql-17-postgis-3 postgresql-17-postgis-3-scripts \
    && rm -rf /var/lib/apt/lists/*
