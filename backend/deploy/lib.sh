# Helpers shared by redeploy.sh and rollback.sh. Sourced, not executed.
#
# Everything here runs from backend/, where compose.yml and .env live.

# `DOCKER="sudo docker"` for a host where the deploy user is not in the
# docker group. Left unquoted where used so that form splits into two words.
DOCKER=${DOCKER:-docker}
BACKUP_DIR=${BACKUP_DIR:-backups}
BACKUP_KEEP=${BACKUP_KEEP:-10}
DEPLOY_LOG="$BACKUP_DIR/deploy.log"
HEALTH_TIMEOUT=${HEALTH_TIMEOUT:-180}

# Fixed in compose.yml; only the password differs per environment, and
# pg_dump inside the container connects over the local socket without one.
PG_USER=pintrail
PG_DB=pintrail

log() { printf '\033[1m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[33mwarning:\033[0m %s\n' "$*" >&2; }
die() {
  printf '\033[31merror:\033[0m %s\n' "$*" >&2
  exit 1
}

compose() { $DOCKER compose "$@"; }

# Which overlays make up "the system" differs per host (prod, prod with
# Versity, a staging box), so it is not guessed. Compose reads COMPOSE_FILE
# from the environment or from .env natively; the scripts only insist it is
# set, so a deploy can never silently fall back to the development stack.
require_compose_file() {
  if [[ -z ${COMPOSE_FILE:-} ]] && ! grep -qs '^COMPOSE_FILE=' .env; then
    die "COMPOSE_FILE is not set. Add a line like this to backend/.env:
    COMPOSE_FILE=compose.yml:compose.prod.yml:compose.versity.yml"
  fi
}

require_clean_tree() {
  git diff --quiet && git diff --cached --quiet ||
    die "the working tree has uncommitted changes; a deploy must match a commit exactly"
}

# A branch name deploys what the remote has, not a stale local copy of it.
resolve_ref() {
  local ref=$1
  git rev-parse --verify --quiet "origin/$ref^{commit}" ||
    git rev-parse --verify --quiet "$ref^{commit}" ||
    die "unknown ref: $ref"
}

postgres_running() {
  [[ -n $(compose ps --quiet --status running postgres 2>/dev/null) ]]
}

# Prints the path of the new dump. The dump is written under a temporary name
# and renamed only once pg_dump succeeds, so a failed dump never looks usable.
# Pass "keep-all" as the second argument to skip pruning -- rollback does,
# since pruning could delete the very dump it is about to restore.
backup_database() {
  local label=$1 prune=${2:-prune}
  mkdir -p "$BACKUP_DIR"
  local file="$BACKUP_DIR/pintrail-$(date -u +%Y%m%dT%H%M%SZ)-$label.dump"

  # Dumps hold password hashes and user content.
  (umask 077 && compose exec -T postgres pg_dump -U "$PG_USER" -d "$PG_DB" -Fc >"$file.partial") ||
    { rm -f "$file.partial"; die "database backup failed"; }
  [[ -s $file.partial ]] || { rm -f "$file.partial"; die "database backup is empty"; }
  mv "$file.partial" "$file"

  # Oldest first out; the deploy log keeps pointing at names, which is fine --
  # rollback checks the file still exists.
  [[ $prune == keep-all ]] ||
    find "$BACKUP_DIR" -maxdepth 1 -name 'pintrail-*.dump' -printf '%T@ %p\n' |
    sort -rn | tail -n +$((BACKUP_KEEP + 1)) | cut -d' ' -f2- | xargs -r rm --

  printf '%s\n' "$file"
}

# Blocks until the service's container healthcheck passes. Services without a
# healthcheck only need to be running.
wait_for_service() {
  local service=$1 deadline=$((SECONDS + HEALTH_TIMEOUT)) id status
  while ((SECONDS < deadline)); do
    id=$(compose ps --quiet "$service")
    if [[ -n $id ]]; then
      status=$($DOCKER inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$id")
      case $status in
        healthy | running) return 0 ;;
        unhealthy | exited | dead) break ;;
      esac
    fi
    sleep 3
  done
  compose logs --tail 50 "$service" >&2 || true
  die "$service did not become healthy (last status: ${status:-no container})"
}

# One line per deploy: time, previous commit, deployed commit, backup taken.
record_deploy() {
  mkdir -p "$BACKUP_DIR"
  printf '%s\t%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$1" "$2" "${3:--}" >>"$DEPLOY_LOG"
}
