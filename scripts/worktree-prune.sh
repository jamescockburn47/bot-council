#!/usr/bin/env bash
# Prune stale Claude Code worktrees under .claude/worktrees/.
#
# Claude Code spawns a fresh git worktree per session under
# `<repo>/.claude/worktrees/<name>/`. Those accumulate over time and — per
# `CLAUDE.md` Operational Lessons — must not be allowed to accrete without
# bound. This script removes them safely.
#
# Always kept:
#   - `reverent-goldwasser` (canonical main checkout used by `ship.sh`)
#   - the current worktree (so the script can be run from inside one)
#   - any worktree whose branch has an open PR (via `gh pr list`)
#
# Everything else is removed, then `git worktree prune` tidies the refs.
#
# Usage:
#   ./scripts/worktree-prune.sh        # dry run — lists what would go
#   ./scripts/worktree-prune.sh --yes  # actually remove
#
# Complement to `scripts/branch-cleanup.ps1`, which prunes local branches
# whose upstream is `[gone]` (different concern — branches, not worktrees).

set -euo pipefail

DRY_RUN=1
if [[ "${1:-}" == "--yes" ]]; then
    DRY_RUN=0
fi

# Resolve the main checkout (first entry from `git worktree list`). Using
# `sed` instead of `awk '{print $2}'` so paths with spaces (Windows /
# "LQ projects/") survive intact.
MAIN_CHECKOUT="$(git worktree list --porcelain | sed -n 's/^worktree //p' | head -1)"
WORKTREES_DIR="${MAIN_CHECKOUT}/.claude/worktrees"

if [[ ! -d "$WORKTREES_DIR" ]]; then
    echo "no $WORKTREES_DIR — nothing to prune"
    exit 0
fi

CURRENT_NAME="$(basename "$(git rev-parse --show-toplevel)")"
declare -a KEEP_NAMES=("reverent-goldwasser" "$CURRENT_NAME")

open_pr_branches=""
if command -v gh >/dev/null 2>&1; then
    open_pr_branches="$(gh pr list --state open --json headRefName --jq '.[].headRefName' 2>/dev/null || true)"
fi

removed=0
kept=0
for wt in "$WORKTREES_DIR"/*/; do
    [[ -d "$wt" ]] || continue
    name="$(basename "$wt")"

    keep=0
    for k in "${KEEP_NAMES[@]}"; do
        if [[ "$name" == "$k" ]]; then keep=1; break; fi
    done
    if [[ $keep -eq 1 ]]; then
        echo "keep    $name  (anchor)"
        kept=$((kept + 1))
        continue
    fi

    branch="$(git -C "$wt" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")"
    if [[ -n "$branch" && -n "$open_pr_branches" ]] \
       && echo "$open_pr_branches" | grep -Fxq "$branch"; then
        echo "keep    $name  (open PR on $branch)"
        kept=$((kept + 1))
        continue
    fi

    if [[ $DRY_RUN -eq 1 ]]; then
        echo "remove  $name  ($branch)  [dry-run]"
    else
        echo "remove  $name  ($branch)"
        git -C "$MAIN_CHECKOUT" worktree remove --force "$wt" 2>/dev/null \
            || { echo "        (git refused, forcing rm)"; rm -rf "$wt"; }
    fi
    removed=$((removed + 1))
done

if [[ $DRY_RUN -eq 0 ]]; then
    git -C "$MAIN_CHECKOUT" worktree prune
    echo "pruned $removed, kept $kept."
else
    echo "--- DRY RUN: $removed to remove, $kept to keep. Re-run with --yes to apply."
fi
