#!/usr/bin/env bash
# Runbook command-rot gate (operator-legibility spec Part 4): every
# ./scripts/<name>.sh named in docs/RUNBOOK.md must exist. Steps that are
# not repo scripts are tagged [manual] and exempt by construction (they
# never match the pattern).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
fail=0
while IFS= read -r script; do
  if [[ ! -f "$ROOT/$script" ]]; then
    echo "::error file=docs/RUNBOOK.md::RUNBOOK names $script which does not exist — update the runbook or restore the script."
    fail=1
  fi
done < <(grep -oE '\./scripts/[a-z0-9-]+\.sh' "$ROOT/docs/RUNBOOK.md" | sed 's|^\./||' | sort -u)
if (( fail == 0 )); then
  echo "runbook gate: OK (all named scripts exist)"
fi
exit $fail
