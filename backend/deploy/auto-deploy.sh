#!/usr/bin/env bash
# Deploys origin/main when it has moved and CI passed on it. Meant to run
# every few minutes from the systemd timer in deploy/systemd/; safe to run
# by hand too.
#
#   deploy/auto-deploy.sh               check once, deploy if needed
#   deploy/auto-deploy.sh --dry-run     say what would happen, change nothing
#
# Each run:
#   1. takes a lock, so two runs (or a run and a manual deploy started the
#      same way) never overlap
#   2. fetches origin/main and compares it with the last deployed commit in
#      backups/deploy.log
#   3. asks GitHub whether the CI workflow passed on that commit; if CI is
#      still running it does nothing and tries again next time
#   4. runs deploy/redeploy.sh --yes --ref <commit>, which backs up, migrates,
#      replaces the containers and waits for the health checks
#
# A commit that was ever deployed before is not deployed again
# automatically. That keeps a manual rollback in place: after
# deploy/rollback.sh, origin/main still points at the commit you rolled back
# from, and redeploying it every five minutes would undo the rollback. The
# next new commit on main deploys as usual.
#
# To pause automatic deploys (maintenance, a risky migration to run by hand):
#   touch backups/auto-deploy.paused        and rm it to resume
#
# A commit whose CI failed, or whose deploy failed, is recorded in
# backups/auto-deploy.state and never retried automatically: a failed
# migration should not be re-run every five minutes. The next commit on main
# is tried as usual. Nothing is ever rolled back automatically; see
# deploy/rollback.sh.
#
# Settings, read from backend/.env (all optional):
#   AUTODEPLOY_GITHUB_TOKEN   token for the GitHub API. Without one, GitHub
#                             allows 60 requests an hour per IP, shared with
#                             everyone behind the same address. A fine-grained
#                             token with read access to Actions is enough;
#                             add Issues: write to get failure issues.
#   AUTODEPLOY_ISSUES=1       open a GitHub issue when CI fails on main or a
#                             deploy fails (needs the token above)
#   AUTODEPLOY_REPO           default pintrail/pintrail
#   AUTODEPLOY_WORKFLOW       default ci.yml

set -Eeuo pipefail

main() {
  local dry_run=0
  while (($#)); do
    case $1 in
      --dry-run) dry_run=1; shift ;;
      -h | --help) sed -n '2,/^$/s/^# \{0,1\}//p' "$0"; exit 0 ;;
      *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
  done

  cd "$(dirname "${BASH_SOURCE[0]}")/.."
  # shellcheck source=lib.sh
  source deploy/lib.sh

  local repo workflow
  repo=$(setting AUTODEPLOY_REPO pintrail/pintrail)
  workflow=$(setting AUTODEPLOY_WORKFLOW ci.yml)
  token=$(setting AUTODEPLOY_GITHUB_TOKEN "")
  issues=$(setting AUTODEPLOY_ISSUES 0)
  STATE_FILE="$BACKUP_DIR/auto-deploy.state"

  mkdir -p "$BACKUP_DIR"
  exec 9>"$BACKUP_DIR/.auto-deploy.lock"
  flock -n 9 || { log "another deploy is running; skipping this run"; exit 0; }

  if [[ -e $BACKUP_DIR/auto-deploy.paused ]]; then
    exit 0 # paused on purpose; see --help
  fi

  git fetch --quiet --prune origin main
  local target deployed
  target=$(git rev-parse origin/main)
  deployed=$(last_deployed)

  if [[ $target == "$deployed" ]]; then
    exit 0 # up to date; stay quiet so the journal is not a wall of no-ops
  fi
  if grep -qs "^$target" "$STATE_FILE"; then
    exit 0 # already gave up on this commit; reported when it happened
  fi
  if [[ -s $DEPLOY_LOG ]] && cut -f3 "$DEPLOY_LOG" | grep -qx "$target"; then
    exit 0 # deployed before and since rolled back from; leave it to a person
  fi

  log "origin/main is ${target:0:10}; deployed is ${deployed:0:10}"

  local ci
  ci=$(ci_result "$repo" "$workflow" "$target")
  case $ci in
    success) ;;
    pending)
      log "CI has not finished on ${target:0:10}; will check again next run"
      exit 0
      ;;
    none)
      log "no CI run found for ${target:0:10} yet; will check again next run"
      exit 0
      ;;
    *)
      warn "CI ended '$ci' on ${target:0:10}; not deploying it"
      ((dry_run)) && exit 0
      give_up "$target" "ci-$ci"
      report "$repo" "CI failed on main at ${target:0:10}; not deployed" \
        "The automatic deploy skipped ${target} because the CI workflow ended '$ci'. The site is still on ${deployed:0:10}. Fix forward on main; the next green commit deploys."
      exit 0
      ;;
  esac

  if ((dry_run)); then
    log "would run: deploy/redeploy.sh --yes --ref $target"
    exit 0
  fi

  log "deploying ${target:0:10}"
  local output rc=0
  output=$(mktemp)
  # --no-fetch: just fetched, and deploying exactly the commit CI checked.
  deploy/redeploy.sh --yes --no-fetch --ref "$target" 2>&1 | tee "$output" || rc=${PIPESTATUS[0]}

  if ((rc != 0)); then
    give_up "$target" "deploy-failed"
    report "$repo" "Automatic deploy of ${target:0:10} failed" \
      "deploy/redeploy.sh exited $rc while deploying ${target}. The last lines of its output are below. If it failed before replacing the containers, the site is still on ${deployed:0:10}; otherwise see deploy/rollback.sh.

\`\`\`
$(tail -n 40 "$output")
\`\`\`"
    rm -f "$output"
    exit "$rc"
  fi
  rm -f "$output"
  log "deployed ${target:0:10}"
}

# Globals so helpers can read them without passing them everywhere.
token=
issues=0
STATE_FILE=

# Reads KEY=value from the environment first, then backend/.env, without
# sourcing .env (it holds secrets and is not written as shell).
setting() {
  local key=$1 default=$2 value
  value=${!key:-}
  if [[ -z $value ]]; then
    value=$(sed -n "s/^$key=//p" .env 2>/dev/null | tail -n 1)
    value=${value%\"}
    value=${value#\"}
  fi
  printf '%s\n' "${value:-$default}"
}

last_deployed() {
  [[ -s $DEPLOY_LOG ]] || { git rev-parse HEAD; return; }
  tail -n 1 "$DEPLOY_LOG" | cut -f3
}

give_up() {
  printf '%s\t%s\t%s\n' "$1" "$2" "$(date -u +%FT%TZ)" >>"$STATE_FILE"
}

github() {
  local args=(-fsSL --max-time 20 -H "Accept: application/vnd.github+json")
  [[ -n $token ]] && args+=(-H "Authorization: Bearer $token")
  curl "${args[@]}" "$@"
}

# Prints success, pending, none, or the failed run's conclusion.
ci_result() {
  local repo=$1 workflow=$2 sha=$3 json
  json=$(github "https://api.github.com/repos/$repo/actions/workflows/$workflow/runs?head_sha=$sha&event=push&per_page=1") ||
    die "could not reach the GitHub API to check CI"
  python3 -c '
import json, sys
runs = json.load(sys.stdin).get("workflow_runs", [])
if not runs:
    print("none")
elif runs[0]["status"] != "completed":
    print("pending")
else:
    print(runs[0]["conclusion"] or "unknown")
' <<<"$json"
}

# Logs always (the systemd journal keeps it); opens an issue when enabled.
report() {
  local repo=$1 title=$2 body=$3
  warn "$title"
  [[ $issues == 1 && -n $token ]] || return 0
  local payload
  payload=$(python3 -c 'import json, sys; print(json.dumps({"title": sys.argv[1], "body": sys.argv[2], "labels": ["deploy"]}))' "$title" "$body")
  github -X POST "https://api.github.com/repos/$repo/issues" -d "$payload" >/dev/null ||
    warn "could not open a GitHub issue about this"
}

# Parsed in full before running, because redeploy.sh checks out a new commit
# and may replace this file while it runs (see the note in redeploy.sh).
main "$@"
exit
