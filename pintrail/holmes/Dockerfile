FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=en_US.UTF-8 \
    TERM=xterm-256color

# ── Locale + base packages ────────────────────────────────────────────────────
RUN apt-get update && apt-get install -y --no-install-recommends \
        locales \
        bash bash-completion \
        curl wget git \
        jq \
        build-essential ca-certificates gnupg \
        unzip zip tar gzip xz-utils \
        less man-db sudo tmux watch \
        python3 python3-pip python3-venv python3-dev \
        postgresql-client \
        redis-tools \
        httpie \
        ripgrep \
        fd-find \
        netcat-openbsd \
        dnsutils \
        iputils-ping \
        lsof \
        procps \
    && locale-gen en_US.UTF-8 \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && rm -rf /var/lib/apt/lists/*

# ── bat (batcat on Ubuntu — alias to bat) ─────────────────────────────────────
RUN apt-get update && apt-get install -y --no-install-recommends bat \
    && ln -sf /usr/bin/batcat /usr/local/bin/bat \
    && rm -rf /var/lib/apt/lists/*

# ── lsd ───────────────────────────────────────────────────────────────────────
RUN set -eux; \
    DPKG_ARCH=$(dpkg --print-architecture); \
    LSD_VER=$(curl -fsSL "https://api.github.com/repos/lsd-rs/lsd/releases/latest" \
              | jq -r .tag_name | tr -d 'v'); \
    curl -fsSL "https://github.com/lsd-rs/lsd/releases/download/v${LSD_VER}/lsd_${LSD_VER}_${DPKG_ARCH}.deb" \
         -o /tmp/lsd.deb \
    && dpkg -i /tmp/lsd.deb \
    && rm /tmp/lsd.deb

# ── yq (YAML processor, complements jq) ──────────────────────────────────────
RUN set -eux; \
    DPKG_ARCH=$(dpkg --print-architecture); \
    case "$DPKG_ARCH" in \
        amd64) YQ_ARCH="amd64" ;; \
        arm64) YQ_ARCH="arm64" ;; \
        *) echo "Unsupported arch: $DPKG_ARCH" && exit 1 ;; \
    esac; \
    curl -fsSL "https://github.com/mikefarah/yq/releases/latest/download/yq_linux_${YQ_ARCH}" \
         -o /usr/local/bin/yq \
    && chmod +x /usr/local/bin/yq

# ── Node.js (LTS) ─────────────────────────────────────────────────────────────
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y nodejs \
    && npm install -g npm@latest \
    && rm -rf /var/lib/apt/lists/*

# ── k6 ────────────────────────────────────────────────────────────────────────
RUN set -eux; \
    DPKG_ARCH=$(dpkg --print-architecture); \
    case "$DPKG_ARCH" in \
        amd64) K6_ARCH="amd64" ;; \
        arm64) K6_ARCH="arm64" ;; \
        *) echo "Unsupported arch: $DPKG_ARCH" && exit 1 ;; \
    esac; \
    K6_VER=$(curl -fsSL "https://api.github.com/repos/grafana/k6/releases/latest" \
             | jq -r .tag_name); \
    curl -fsSL "https://github.com/grafana/k6/releases/download/${K6_VER}/k6-${K6_VER}-linux-${K6_ARCH}.tar.gz" \
         | tar -xz -C /tmp \
    && mv "/tmp/k6-${K6_VER}-linux-${K6_ARCH}/k6" /usr/local/bin/k6 \
    && rm -rf "/tmp/k6-${K6_VER}-linux-${K6_ARCH}"

# ── Shell conveniences ────────────────────────────────────────────────────────
COPY .bashrc_extras /tmp/.bashrc_extras
RUN cat /tmp/.bashrc_extras >> /root/.bashrc

WORKDIR /workspace

# Stay alive so `docker compose exec holmes bash` works
CMD ["sleep", "infinity"]
