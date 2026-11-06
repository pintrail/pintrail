#!/usr/bin/env bash
# Rebuilds Valhalla's routing data from a fresh OpenStreetMap extract and
# swaps it in, keeping the live data serving until the new build is done.
# Run monthly by deploy/systemd/pintrail-routing-refresh.timer; safe by hand.
#
#   deploy/routing-refresh.sh             build, swap, check, keep the old copy
#   deploy/routing-refresh.sh --rollback  go back to the previous copy
#
# In the valhalla volume:
#   current/    what the running service uses
#   next/       the build in progress (deleted before each build)
#   previous/   the copy before the last swap, kept for --rollback
#   rolled-back/, failed/   set aside by --rollback or a failed swap
#
# One copy is about 530 MB for Massachusetts (the first build on a fresh
# volume keeps another 670 MB of build leftovers until its first refresh), so
# allow about 2 GB: current, previous, and next while building.
#
# Steps, each stopping the refresh on failure (the live copy is untouched
# until step 3):
#   1. build into next/ with a one-off container (downloads the extract)
#   2. check the build produced a tile archive
#   3. swap: current -> previous, next -> current; restart the service
#   4. wait for /status; if it never answers, swap back and restart
#
# Environment: COMPOSE_FILE and COMPOSE_PROFILES (from backend/.env; the
# routing profile must be on), VALHALLA_THREADS, HEALTH_TIMEOUT.

set -Eeuo pipefail

main() {
  local rollback=0
  while (($#)); do
    case $1 in
      --rollback) rollback=1; shift ;;
      -h | --help) sed -n '2,/^$/s/^# \{0,1\}//p' "$0"; exit 0 ;;
      *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
  done

  cd "$(dirname "${BASH_SOURCE[0]}")/.."
  # shellcheck source=lib.sh
  source deploy/lib.sh
  require_compose_file

  mkdir -p "$BACKUP_DIR"
  exec 9>"$BACKUP_DIR/.routing-refresh.lock"
  if command -v flock >/dev/null; then
    flock -n 9 || die "another routing refresh is running"
  fi

  if ((rollback)); then
    in_volume '[ -d previous ] || { echo "no previous copy to go back to" >&2; exit 1; }
               rm -rf rolled-back && mv current rolled-back && mv previous current'
    restart_and_wait || die "the previous copy did not come up either; see: docker compose logs valhalla"
    log "rolled back to the previous routing data"
    return
  fi

  log "building new routing data (this takes a while)"
  in_volume 'rm -rf next'
  # serve_tiles=False: build, then exit. force_rebuild: always from the
  # freshly downloaded extract, never from a leftover archive.
  compose run --rm --no-deps \
    -e path_extension=next -e serve_tiles=False -e force_rebuild=True \
    valhalla ||
    die "the build failed; the live routing data was not touched"

  in_volume '[ -s next/valhalla_tiles.tar ]' ||
    die "the build produced no tile archive; the live routing data was not touched"
  # The generated config names next/ in its paths. Drop it so the service
  # writes a fresh one for current/ on start; nothing else needs rebuilding.
  # The service reads the tile archive, so the loose tiles and the extract
  # are dead weight (about 670 MB for Massachusetts).
  in_volume 'rm -rf next/valhalla.json next/valhalla_tiles next/*.osm.pbf'

  log "swapping in the new data"
  in_volume 'rm -rf previous && mv current previous && mv next current'
  if ! restart_and_wait; then
    warn "the new data did not come up; putting the old copy back"
    in_volume 'rm -rf failed && mv current failed && mv previous current'
    restart_and_wait || warn "the old copy did not come up either; see: docker compose logs valhalla"
    die "routing refresh failed; the new build is kept in failed/ for inspection"
  fi
  touch "$BACKUP_DIR/routing-refreshed-at"
  log "routing data refreshed ($(in_volume 'du -sh current | cut -f1'))"
}

# Runs a shell snippet inside the valhalla volume, at /custom_files.
in_volume() {
  compose run --rm --no-deps -T --entrypoint sh -w /custom_files valhalla -c "$1"
}

restart_and_wait() {
  compose up -d --no-deps --force-recreate valhalla >/dev/null
  wait_for_service valhalla
}

main "$@"
exit
