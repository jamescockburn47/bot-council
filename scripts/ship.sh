#!/usr/bin/env bash
# ship.sh — one-command deploy of the LQ Bot Council backend + frontend to EVO.
#
# Preflight -> build frontend locally -> sync src + build to EVO -> save
# previous binary as .prev -> cargo build --release on EVO -> systemctl restart
# -> poll /api/health until green -> smoke public path.
#
# Exits non-zero and prints the stage that failed if any step does not succeed.
# On success, writes the deployed SHA to ~/bot-council/.last-known-good-sha
# on EVO so rollback.sh knows where to pin back to.
#
# Usage:
#   ./scripts/ship.sh            # deploys current checked-out commit
#   ./scripts/ship.sh --skip-ci  # skips the frontend build (use if you've
#                                # already run `npm run build` and just want
#                                # to redeploy)
#
# Overrideable environment:
#   SSH_KEY   (default: C:/Users/James/.ssh/id_ed25519)
#   EVO_HOST  (default: james@100.90.66.54)
#   PUBLIC_URL (default: https://lqcouncil.com; becomes just the EVO path if
#              you're pre-Cloudflare and want Tailscale-Funnel smoke only —
#              set to https://james-nucbox-evo-x2.taila41c86.ts.net)

set -euo pipefail

KEY="${SSH_KEY:-C:/Users/James/.ssh/id_ed25519}"
HOST="${EVO_HOST:-james@100.90.66.54}"
REMOTE="~/bot-council"
# Default to the *api* subdomain during the Vercel-to-Cloudflare transition:
# lqcouncil.com is served by Vercel and may be stale or intentionally broken
# as the frontend migrates. api.lqcouncil.com is the Vercel proxy that routes
# straight through to EVO and is the authoritative public-path check until
# Phase E (Cloudflare) completes. After Phase F this can become
# https://lqcouncil.com.
PUBLIC_URL="${PUBLIC_URL:-https://api.lqcouncil.com}"
SKIP_FRONTEND=0

for arg in "$@"; do
  case "$arg" in
    --skip-ci|--skip-frontend) SKIP_FRONTEND=1 ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

stage() {
  printf '\n>>> %s\n' "$1"
}

fail() {
  printf '\n!!! FAILED at stage: %s\n' "$1" >&2
  exit 3
}

# Stage 1: preflight — branch, working tree, ssh reachability.
stage "1/7 preflight"
BRANCH=$(git rev-parse --abbrev-ref HEAD)
SHA=$(git rev-parse HEAD)
if [[ "$BRANCH" != "main" ]]; then
  echo "refusing to ship from branch '$BRANCH' — switch to main first" >&2
  fail "preflight: wrong branch"
fi
if [[ -n "$(git status --porcelain)" ]]; then
  echo "working tree has uncommitted changes — commit or stash first" >&2
  git status --short >&2
  fail "preflight: dirty tree"
fi
if ! ssh -o BatchMode=yes -o ConnectTimeout=5 -i "$KEY" "$HOST" true 2>/dev/null; then
  echo "EVO unreachable at $HOST via SSH" >&2
  fail "preflight: ssh"
fi
echo "branch=main  sha=$SHA  ssh=ok"

# Stage 2: env-file sanity on EVO (local model URL, bot token key, clerk issuer).
stage "2/7 env-file preflight on EVO"
ssh -i "$KEY" "$HOST" '
  required=(APP__MODELS__MINIMAX_BASE_URL APP__AUTH__BOT_TOKEN_KEY APP__AUTH__CLERK_ISSUER APP__AUTH__CLERK_PUBLISHABLE_KEY)
  missing=0
  for k in "${required[@]}"; do
    if ! sudo grep -qE "^${k}=." /etc/bot-council.env; then
      echo "  missing or empty: $k" >&2
      missing=$((missing+1))
    fi
  done
  exit $missing
' || fail "env-file: required APP__* keys missing in /etc/bot-council.env"
echo "env-file ok"

# Stage 3: build frontend locally (svelte-check + vite build).
if [[ "$SKIP_FRONTEND" == "0" ]]; then
  stage "3/7 frontend build"
  ( cd frontend && npm ci --silent && npm run build ) || fail "frontend build"
  echo "frontend/build/ ready"
else
  stage "3/7 frontend build skipped (--skip-ci)"
fi

# Stage 4: sync sources + frontend build to EVO.
# scp here rather than rsync because rsync isn't installed on Windows by default;
# the trade-off is we overwrite everything rather than doing diff-transfer.
stage "4/7 sync source + frontend/build to EVO"
scp -q -i "$KEY" -r \
  src tests config migrations Cargo.toml Cargo.lock \
  "$HOST:$REMOTE/" || fail "scp src"
scp -q -i "$KEY" -r frontend/build "$HOST:$REMOTE/frontend/" || fail "scp frontend/build"
echo "sync ok"

# Stage 5: on EVO — save previous binary as .prev, build release, write
# SENTRY_RELEASE, restart.
stage "5/7 build release on EVO + restart"
ssh -i "$KEY" "$HOST" "
  set -e
  source ~/.cargo/env
  cd $REMOTE
  if [[ -x target/release/bot-council ]]; then
    cp -f target/release/bot-council target/release/bot-council.prev
  fi
  cargo build --release
  sudo sed -i -E '/^SENTRY_RELEASE=/d' /etc/bot-council.env
  echo 'SENTRY_RELEASE=$SHA' | sudo tee -a /etc/bot-council.env >/dev/null
  sudo systemctl restart bot-council
" || fail "build + restart"
echo "rebuild + restart ok"

# Stage 6: health poll — 30 seconds max.
stage "6/7 health poll (30s max)"
for i in $(seq 1 15); do
  if ssh -i "$KEY" "$HOST" 'curl -fsS http://127.0.0.1:3100/api/health' >/dev/null 2>&1; then
    echo "healthy on attempt $i"
    ssh -i "$KEY" "$HOST" "echo $SHA > $REMOTE/.last-known-good-sha"
    break
  fi
  sleep 2
  if [[ $i -eq 15 ]]; then
    echo "tailing journal for diagnosis:" >&2
    ssh -i "$KEY" "$HOST" 'journalctl -u bot-council -n 30 --no-pager' >&2
    fail "health poll"
  fi
done

# Stage 7: public smoke test. Checks actual JSON content, not just HTTP 200 —
# a misconfigured frontend rewrite can return 200 with an HTML SPA shell.
stage "7/7 public smoke"
# Try /api/health first; if that's HTML, also try /health (backward-compat path
# that still works until Phase F drops the Vercel proxy).
probe() {
  local url=$1
  local body
  body=$(curl -fsS --max-time 10 "$url" 2>/dev/null || true)
  if [[ "$body" == *'"status":"ok"'* ]]; then
    echo "public $url: {\"status\":\"ok\"}"
    return 0
  fi
  return 1
}
if probe "$PUBLIC_URL/api/health" || probe "$PUBLIC_URL/health"; then
  true
else
  echo "WARNING: public smoke failed at $PUBLIC_URL — deploy is live on EVO but the public path may be stale. Check api.lqcouncil.com proxy rewrites." >&2
fi

echo
echo "SHIPPED $SHA"
echo "  SENTRY_RELEASE=$SHA written to /etc/bot-council.env"
echo "  .last-known-good-sha updated on EVO"
echo "  rollback available via: ./scripts/rollback.sh"
