#!/usr/bin/env bash
# Sync source tree to EVO X2 and run a cargo command there.
# Usage:
#   ./scripts/sync-evo.sh            # cargo test (default)
#   ./scripts/sync-evo.sh check      # cargo check --tests
#   ./scripts/sync-evo.sh build      # cargo build --release
#   ./scripts/sync-evo.sh run        # cargo run
#   ./scripts/sync-evo.sh test       # cargo test (explicit)
#   ./scripts/sync-evo.sh restart    # sync + release build + sudo systemctl restart bot-council
#   ./scripts/sync-evo.sh "<raw>"    # any raw cargo-style command after sync
#
# Honours CARGO_HOST env var override if you ever change the IP.

set -euo pipefail

KEY="${SSH_KEY:-C:/Users/James/.ssh/id_ed25519}"
HOST="${CARGO_HOST:-james@100.90.66.54}"
REMOTE="~/bot-council"

action="${1:-test}"

echo ">>> scp src tests config migrations Cargo.toml Cargo.lock -> ${HOST}:${REMOTE}"
scp -q -i "${KEY}" -r \
  src tests config migrations Cargo.toml Cargo.lock \
  "${HOST}:${REMOTE}/"

case "${action}" in
  test)
    cmd="cargo test"
    ;;
  check)
    cmd="cargo check --tests"
    ;;
  build)
    cmd="cargo build --release"
    ;;
  run)
    cmd="cargo run"
    ;;
  restart)
    # Capture the local HEAD SHA so Sentry release tagging tracks the
    # exact commit being deployed. The remote side refreshes the
    # SENTRY_RELEASE line in /etc/bot-council.env before restarting.
    sha="$(git rev-parse HEAD)"
    cmd="cargo build --release && \
         sudo sed -i -E '/^SENTRY_RELEASE=/d' /etc/bot-council.env && \
         echo 'SENTRY_RELEASE=${sha}' | sudo tee -a /etc/bot-council.env >/dev/null && \
         sudo systemctl restart bot-council"
    ;;
  *)
    # Treat the argument as a raw cargo-style tail.
    cmd="${action}"
    ;;
esac

echo ">>> ${HOST}: ${cmd}"
ssh -i "${KEY}" "${HOST}" "source ~/.cargo/env && cd ${REMOTE} && ${cmd}"
