# Both binaries from one Dockerfile.
#
# The workspace compiles once in a shared builder stage and two thin runtime
# stages copy out the binary each needs:
#
#   docker build --target api    -t pintrail-api    .
#   docker build --target worker -t pintrail-worker .
#
# Building them separately would compile the shared dependency graph -- the
# AWS SDK dominates it -- twice.

# ---------------------------------------------------------------------------
# Builder
# ---------------------------------------------------------------------------
# aws-sdk-s3 requires rustc 1.94.1; pinning an exact version keeps a toolchain
# release from silently changing what ships.
FROM rust:1.97-bookworm AS builder

WORKDIR /build

# Dependencies first, so editing our own source does not re-download and
# rebuild the entire dependency graph. The dummy sources exist only to give
# cargo something compilable per crate.
COPY Cargo.toml Cargo.lock ./
COPY libs/pintrail-storage/Cargo.toml libs/pintrail-storage/
COPY services/pintrail-api/Cargo.toml services/pintrail-api/
COPY services/pintrail-worker/Cargo.toml services/pintrail-worker/

RUN mkdir -p libs/pintrail-storage/src services/pintrail-api/src services/pintrail-worker/src \
    && echo "" > libs/pintrail-storage/src/lib.rs \
    && echo "fn main() {}" > services/pintrail-api/src/main.rs \
    && echo "fn main() {}" > services/pintrail-worker/src/main.rs \
    # build.rs is referenced by pintrail-api's manifest, so it must exist now.
    && echo "fn main() {}" > services/pintrail-api/build.rs \
    && mkdir -p migrations \
    && cargo build --release \
    # Remove the stubs' own artifacts, or cargo considers the real sources
    # already built and ships an empty binary.
    && rm -rf libs/pintrail-storage/src services/pintrail-api/src services/pintrail-worker/src \
    && rm -f target/release/pintrail-api target/release/pintrail-worker \
    && rm -f target/release/deps/pintrail_api* target/release/deps/pintrail_worker* \
    && rm -f target/release/deps/libpintrail_storage*

COPY migrations ./migrations
COPY libs ./libs
COPY services ./services

RUN cargo build --release --locked \
    && strip target/release/pintrail-api target/release/pintrail-worker

# ---------------------------------------------------------------------------
# Runtime base
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime-base

# ca-certificates: TLS to S3. There is deliberately no libssl here -- sqlx and
# the AWS SDK are configured for rustls, so the image carries no OpenSSL.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Never run as root: a media decoder parsing hostile files is the least
# trustworthy code in the system.
RUN useradd --system --create-home --uid 10001 pintrail
USER pintrail
WORKDIR /app

# ---------------------------------------------------------------------------
# API
# ---------------------------------------------------------------------------
FROM runtime-base AS api

COPY --from=builder /build/target/release/pintrail-api /usr/local/bin/pintrail-api

# Migrations and admin templates are compiled into the binary, so nothing else
# needs copying and the image cannot start with a stale set of either.
EXPOSE 8080
ENV BIND_ADDR=0.0.0.0:8080

# Uses the liveness endpoint, which deliberately does not touch the database --
# a database blip should not make an orchestrator kill a healthy API. Written
# with a bare TCP+HTTP request because the image has no curl by design.
HEALTHCHECK --interval=15s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/usr/local/bin/pintrail-api", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/pintrail-api"]

# ---------------------------------------------------------------------------
# Worker
# ---------------------------------------------------------------------------
FROM runtime-base AS worker

USER root

# The media pipeline shells out to these rather than linking them:
#   libheif-examples -> heif-convert, for iPhone HEIC photos
#   poppler-utils    -> pdftoppm, for PDF page-1 thumbnails
# Missing either turns every upload of that kind into a failed attachment with
# "is libheif installed in this image?" on the row.
RUN apt-get update \
    && apt-get install -y --no-install-recommends libheif-examples poppler-utils \
    && rm -rf /var/lib/apt/lists/*

USER pintrail

COPY --from=builder /build/target/release/pintrail-worker /usr/local/bin/pintrail-worker

ENTRYPOINT ["/usr/local/bin/pintrail-worker"]
