#!/usr/bin/env bash
# wt-remove.sh — Remove a git worktree and its local branch.
#
# Usage:
#   ./support/scripts/wt-remove.sh <branch-name>
#
# The worktree at ~/Documents/d-token-worktrees/<branch-name> is removed.
# The local branch is deleted only if it has been merged into main;
# otherwise it is preserved and a warning is printed.

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
