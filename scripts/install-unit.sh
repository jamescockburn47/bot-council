#!/usr/bin/env bash
# Install or refresh the bot-council systemd unit on EVO.
# Run over SSH, e.g.:
#   scp -i ~/.ssh/id_ed25519 deploy/bot-council.service scripts/install-unit.sh \
#       james@100.90.66.54:/tmp/
#   ssh -i ~/.ssh/id_ed25519 james@100.90.66.54 \
#       'sudo bash /tmp/install-unit.sh /tmp/bot-council.service'
set -euo pipefail

UNIT_SRC=${1:-bot-council.service}
UNIT_DST=/etc/systemd/system/bot-council.service

if [[ ! -f "$UNIT_SRC" ]]; then
  echo "error: unit file not found: $UNIT_SRC" >&2
  exit 1
fi

install -m 0644 -o root -g root "$UNIT_SRC" "$UNIT_DST"
systemctl daemon-reload
systemctl enable bot-council
systemctl restart bot-council
sleep 2
systemctl is-active bot-council
curl -sS http://127.0.0.1:3100/health
echo
