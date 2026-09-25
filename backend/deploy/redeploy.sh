#!/usr/bin/env bash
# Redeploys the stack at a given commit, with the database backed up and
# migrated as explicit steps.
#
#   deploy/redeploy.sh                  deploy origin/main
#   deploy/redeploy.sh --ref v1.4.0     deploy a tag, branch, or commit
#
# Options:
#   --ref <ref>       what to deploy (default: main). Branch names resolve to
#                     origin/<branch>, so the remote's version is deployed.
#   --no-fetch        do not `git fetch` first (used by rollback.sh)
#   --skip-backup     do not dump the database first. Only for when a backup
#                     was just taken some other way.
#   -y, --yes         do not ask for confirmation
#
# Environment: COMPOSE_FILE (required; set it in backend/.env), DOCKER,
# BACKUP_DIR, BACKUP_KEEP, HEALTH_TIMEOUT -- see deploy/lib.sh.
#
# Order matters, and each step stops the deploy on failure:
#   1. check out the commit        4. run migrations (new image, one-off)
#   2. build images                5. replace the running containers
#   3. back up the database        6. wait for the API healthcheck
# The old containers keep serving through steps 1-4, so a failed build,
# backup, or migration leaves the site up on the previous version.

set -Eeuo pipefail

main() {
  local ref=main fetch=1 backup=1 assume_yes=0
  while (($#)); do
    case $1 in
      --ref) ref=${2:?--ref needs a value}; shift 2 ;;
      --no-fetch) fetch=0; shift ;;
      --skip-backup) backup=0; shift ;;
      -y | --yes) assume_yes=1; shift ;;
      -h | --help) sed -n '2,/^$/s/^# \{0,1\}//p' "$0"; exit 0 ;;
      *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
  done

  cd "$(dirname "${BASH_SOURCE[0]}")/.."
  # shellcheck source=lib.sh
  source deploy/lib.sh

  require_compose_file
  require_clean_tree

  local to
  from=$(git rev-parse HEAD)
  if ((fetch)); then
    log "fetching from origin"
    git fetch --quiet --prune --tags origin
  fi
  to=$(resolve_ref "$ref")

  log "deploying $ref: ${from:0:10} -> ${to:0:10}"
  if [[ $from != "$to" ]]; then
    git --no-pager log --oneline --no-decorate -n 20 "$from..$to" | sed 's/^/    /'
    local migrations
    migrations=$(git diff --name-only --diff-filter=A "$from" "$to" -- migrations)
    if [[ -n $migrations ]]; then
      log "new migrations:"
      sed 's/^/    /' <<<"$migrations"
    fi
  else
    log "already at this commit; rebuilding and restarting anyway"
  fi

  if ((!assume_yes)); then
    read -r -p "Continue? [y/N] " answer
    [[ $answer == [yY]* ]] || die "aborted"
  fi

  stage=checkout
  trap on_exit EXIT

  git checkout --quiet --detach "$to"

  stage=build
  log "building images"
  compose build --pull

  stage=backup
  local backup_file=
  if ((backup)); then
    if postgres_running; then
      log "backing up the database"
      backup_file=$(backup_database "${to:0:10}")
      log "backup written to $backup_file"
    else
      warn "postgres is not running, so there is nothing to back up (first deploy?)"
    fi
  fi

  stage=migrate
  log "running migrations"
  compose run --rm api migrate

  stage=restart
  log "replacing containers"
  compose up -d --remove-orphans

  stage=health
  log "waiting for the api healthcheck (up to ${HEALTH_TIMEOUT}s)"
  wait_for_service api
  wait_for_service worker

  stage=done
  record_deploy "$from" "$to" "$backup_file"
  log "deployed ${to:0:10}"
}

# Globals rather than locals of main, because the EXIT trap may run after
# main's frame is gone (e.g. when `die` exits from inside a helper).
stage=
from=

on_exit() {
  local rc=$?
  ((rc == 0)) || [[ $stage == done ]] && return
  echo >&2
  case $stage in
    checkout | build | backup)
      warn "failed during $stage; the running containers were not touched."
      warn "the working tree is now at the new commit; to return it: git checkout ${from:0:10}"
      ;;
    migrate)
      warn "a migration failed; the running containers were not touched."
      warn "postgres runs each migration in a transaction, so a failed one leaves no partial change,"
      warn "but earlier migrations in this deploy may have applied. The old code tolerates that as"
      warn "long as they were additive. Fix forward, or see deploy/rollback.sh --help."
      ;;
    restart | health)
      warn "the new version failed to start. Roll back with: deploy/rollback.sh"
      ;;
  esac
}

# Everything above is parsed before any of it runs. That matters because the
# checkout step replaces this very file: bash reads a script incrementally, and
# without the wrapper it would continue reading from the *new* version at
# whatever byte offset it had reached.
main "$@"
exit
