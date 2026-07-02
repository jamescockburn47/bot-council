#!/usr/bin/env bash
# File-length gate (CLAUDE.md: max 300 lines per file, split before adding).
#
# New files are held to the 300-line limit. Pre-existing offenders are
# grandfathered in scripts/file-length-grandfather.txt with a RATCHET
# ceiling: each may not grow beyond its recorded line count. Shrink one
# below 300 and you can delete its row.
#
# Runs in CI (see .github/workflows/ci.yml) and locally:
#   ./scripts/check-file-length.sh
set -euo pipefail

LIMIT=300
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GRANDFATHER="$ROOT/scripts/file-length-grandfather.txt"

declare -A ceiling
while read -r path max; do
  [[ -z "$path" || "$path" == \#* ]] && continue
  ceiling["$path"]=$max
done < "$GRANDFATHER"

fail=0
while IFS= read -r f; do
  rel="${f#"$ROOT"/}"
  lines=$(wc -l < "$f")
  if [[ -n "${ceiling[$rel]:-}" ]]; then
    if (( lines > ${ceiling[$rel]} )); then
      echo "::error file=$rel::$rel is $lines lines — exceeds its grandfather ceiling of ${ceiling[$rel]}. Split it (target ≤$LIMIT); do not raise the ceiling."
      fail=1
    fi
  elif (( lines > LIMIT )); then
    echo "::error file=$rel::$rel is $lines lines (limit $LIMIT for non-grandfathered files). Split before merging."
    fail=1
  fi
done < <(find "$ROOT/src" -name '*.rs' -type f; find "$ROOT/frontend/src" \( -name '*.svelte' -o -name '*.ts' \) -not -name '*.d.ts' -type f)

if (( fail == 0 )); then
  echo "file-length gate: OK (limit $LIMIT, $(wc -l < "$GRANDFATHER") grandfather entries)"
fi
exit $fail
