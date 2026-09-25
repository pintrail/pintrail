#!/usr/bin/env bash
# Undoes the last deploy by redeploying the commit it replaced.
#
#   deploy/rollback.sh                  code only (the usual case)
#   deploy/rollback.sh --restore-db     code, and the database as it was
#                                       before the last deploy migrated it
#
# Options:
#   --to <ref>        roll back to this commit instead of the one the last
#                     deploy replaced
#   --restore-db      also restore the backup the last deploy took. DESTROYS
#                     every write since that deploy; asks for confirmation.
#   --backup <file>   restore this dump instead (implies --restore-db)
#   -y, --yes         do not ask for confirmation
#
# Code-only rollback works because migrations are written to be additive (add
# a column before using it, drop the old one a release later), and the API
# tolerates database migrations it does not know about. Restore the database
# only when a migration damaged data, not merely because it ran.
#
# Rolling back twice undoes the rollback: each run appends to the deploy log
# (backups/deploy.log) like any other deploy.

set -Eeuo pipefail

main() {
  local target= restore=0 backup_file= assume_yes=0
  while (($#)); do
    case $1 in
      --to) target=${2:?--to needs a value}; shift 2 ;;
      --restore-db) restore=1; shift ;;
      --backup) backup_file=${2:?--backup needs a file}; restore=1; shift 2 ;;
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

  local last=
  [[ -s $DEPLOY_LOG ]] && last=$(tail -n 1 "$DEPLOY_LOG")
  if [[ -z $target ]]; then
    [[ -n $last ]] || die "no deploy recorded in $DEPLOY_LOG; pass --to <ref>"
    target=$(cut -f2 <<<"$last")
  fi
  if ((restore)) && [[ -z $backup_file ]]; then
    backup_file=$(cut -f4 <<<"$last")
    [[ -n $backup_file && $backup_file != - ]] ||
      die "the last deploy took no backup; pass --backup <file> (see ls $BACKUP_DIR)"
  fi
  if ((restore)); then
    [[ -f $backup_file ]] || die "backup not found: $backup_file"
  fi

  log "rolling back to ${target:0:10}"
  if ((restore)); then
    warn "the database will be replaced with $backup_file."
    warn "everything written since that backup (accounts, artifacts, comments) will be lost."
    if ((!assume_yes)); then
      read -r -p "Type 'restore' to continue: " answer
      [[ $answer == restore ]] || die "aborted"
    fi
    restore_database "$backup_file"
    # Just replaced from a backup, and a safety dump was taken beforehand.
    exec deploy/redeploy.sh --ref "$target" --no-fetch --skip-backup --yes
  fi

  local yes=()
  ((assume_yes)) && yes=(--yes)
  exec deploy/redeploy.sh --ref "$target" --no-fetch "${yes[@]}"
}

restore_database() {
  local file=$1

  postgres_running || die "postgres is not running"

  log "taking a safety backup of the current database first"
  local safety
  safety=$(backup_database pre-restore keep-all)
  log "safety backup: $safety"

  # Nothing may hold a connection or write mid-restore.
  log "stopping api and worker"
  compose stop api worker

  # Drop and recreate rather than pg_restore --clean: --clean only drops
  # objects that are in the dump, so a table a later migration created would
  # survive while its migration record did not, and the next deploy would fail
  # trying to create it again.
  log "restoring $file"
  compose exec -T postgres dropdb -U "$PG_USER" --if-exists --force "$PG_DB"
  compose exec -T postgres createdb -U "$PG_USER" "$PG_DB"
  compose exec -T postgres pg_restore -U "$PG_USER" -d "$PG_DB" --no-owner --exit-on-error <"$file" ||
    die "restore failed; the database may be partial. Restore the safety backup with:
    deploy/rollback.sh --backup $safety --to $(git rev-parse --short HEAD)"
}

# See redeploy.sh: parsed in full before the checkout rewrites this file.
main "$@"
exit
