#!/usr/bin/env bash
# rollback.sh — revert the bot-council backend to the previous binary on EVO.
#
# ship.sh saves the currently-running binary as `bot-council.prev` before
# building a new release. This script swaps them back: stops the service,
# moves the broken binary aside, restores .prev, starts the service, polls
# /api/health.
#
# The local source tree is NOT touched; the assumption is that whoever invokes
# rollback will separately `git reset --hard` or `git checkout` to match
# ~/bot-council/.last-known-good-sha on EVO. This script deliberately does
# only ONE job — flip the running binary — so it never has to rebuild.
#
# Usage:
#   ./scripts/rollback.sh
#
# Overrideable environment: same SSH_KEY / EVO_HOST as ship.sh.

set -euo pipefail

KEY="${SSH_KEY:-C:/Users/James/.ssh/id_ed25519}"
HOST="${EVO_HOST:-james@100.90.66.54}"
REMOTE="~/bot-council"

stage() { printf '\n>>> %s\n' "$1"; }
fail() { printf '\n!!! %s\n' "$1" >&2; exit 3; }

stage "checking .prev binary exists on EVO"
if ! ssh -i "$KEY" "$HOST" "test -x $REMOTE/target/release/bot-council.prev"; then
  fail "no bot-council.prev on EVO — nothing to roll back to"
fi

stage "swapping binary back"
ssh -i "$KEY" "$HOST" "
  set -e
  cd $REMOTE
  sudo systemctl stop bot-council
  mv target/release/bot-council target/release/bot-council.broken.\$(date +%Y%m%d-%H%M%S)
  mv target/release/bot-council.prev target/release/bot-council
  sudo systemctl start bot-council
" || fail "swap"

stage "health poll (30s max)"
for i in $(seq 1 15); do
  if ssh -i "$KEY" "$HOST" 'curl -fsS http://127.0.0.1:3100/api/health' >/dev/null 2>&1; then
    echo "healthy on attempt $i"
    LAST=$(ssh -i "$KEY" "$HOST" "cat $REMOTE/.last-known-good-sha 2>/dev/null || echo unknown")
    echo
    echo "ROLLED BACK"
    echo "  previous binary is now live"
    echo "  broken binary preserved as bot-council.broken.* (delete when you're sure)"
    echo "  .last-known-good-sha on EVO: $LAST"
    echo "  suggested: git reset --hard $LAST  (to match local source to what's running)"
    exit 0
  fi
  sleep 2
done

echo "tailing journal for diagnosis:" >&2
ssh -i "$KEY" "$HOST" 'journalctl -u bot-council -n 30 --no-pager' >&2
fail "rollback failed — bot-council did not come back healthy after swap"
