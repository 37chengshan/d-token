#!/usr/bin/env bash
# wt-remove.sh — Remove a git worktree and its local branch.
#
# Usage:
#   ./support/scripts/wt-remove.sh <branch-name>
#
# The worktree at ~/Documents/d-token-worktrees/<branch-name> is removed.
# The local branch is deleted only if it has been merged into main;
# otherwise it is preserved and a warning is printed.
#
# ⚠ DESTRUCTIVE OPERATION SAFETY GATES ⚠
# 1. `worktree remove --force` discards any UNCOMMITTED changes in the
#    worktree. Before running this, confirm the worktree has no uncommitted
#    work: `git -C "$WT_PATH" status`.
# 2. `branch -d` only deletes already-merged branches, but if your `main`
#    is not up-to-date, a merged branch may appear unmerged. Run
#    `git fetch origin main` first if in doubt.
# 3. This script does NOT touch the main checkout or any branch other than
#    `$BRANCH`. It will refuse if the worktree path is missing.
# 4. Never use this to remove a worktree that holds the ONLY copy of
#    uncommitted work — force-remove is unrecoverable.

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <branch-name>" >&2
  exit 1
fi

BRANCH="$1"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WT_ROOT="$HOME/Documents/d-token-worktrees"
WT_PATH="$WT_ROOT/$BRANCH"

if [[ ! -d "$WT_PATH" ]]; then
  echo "✗ Worktree not found: $WT_PATH" >&2
  exit 1
fi

# Safety gate: warn loudly about uncommitted changes before force-remove.
if ! git -C "$WT_PATH" diff --quiet --exit-code 2>/dev/null || \
   ! git -C "$WT_PATH" diff --cached --quiet --exit-code 2>/dev/null; then
  echo "⚠ WARNING: worktree '$BRANCH' has uncommitted changes." >&2
  echo "  Running this script will DESTROY them (force-remove)." >&2
  echo "  Inspect first: git -C \"$WT_PATH\" status" >&2
  read -r -p "Type 'DESTROY' to proceed and lose uncommitted work: " confirm
  if [[ "$confirm" != "DESTROY" ]]; then
    echo "✗ Aborted (uncommitted work preserved)." >&2
    exit 1
  fi
fi

echo "→ Removing worktree: $WT_PATH"
git -C "$REPO_ROOT" worktree remove "$WT_PATH" --force

# Check if branch is merged into main before deleting
if git -C "$REPO_ROOT" merge-base --is-ancestor "$BRANCH" main 2>/dev/null; then
  git -C "$REPO_ROOT" branch -d "$BRANCH" 2>/dev/null || true
  echo "✓ Worktree removed and branch '$BRANCH' deleted (merged into main)"
else
  echo "✓ Worktree removed"
  echo "⚠ Branch '$BRANCH' is not merged into main; branch preserved."
  echo "  To delete manually: git -C \"$REPO_ROOT\" branch -D $BRANCH"
fi
