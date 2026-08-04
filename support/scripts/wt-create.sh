#!/usr/bin/env bash
# wt-create.sh — Create a git worktree for parallel development.
#
# Usage:
#   ./support/scripts/wt-create.sh <branch-name> [base-branch]
#
# Examples:
#   ./support/scripts/wt-create.sh feat/ui-polish main
#   ./support/scripts/wt-create.sh fix/compress-bug
#
# The worktree is created at ~/Documents/d-token-worktrees/<branch-name>.
# Dependencies are installed automatically (pnpm preferred, falls back to npm).
# Rust target/ is per-worktree; sccache is used if available.

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <branch-name> [base-branch]" >&2
  exit 1
fi

BRANCH="$1"
BASE="${2:-main}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WT_ROOT="$HOME/Documents/d-token-worktrees"
WT_PATH="$WT_ROOT/$BRANCH"

# Sanity checks
if [[ ! -d "$REPO_ROOT/.git" ]]; then
  echo "✗ Not a git repository: $REPO_ROOT" >&2
  exit 1
fi

if git -C "$REPO_ROOT" worktree list --porcelain | grep -q "^worktree $WT_PATH$"; then
  echo "✗ Worktree already exists: $WT_PATH" >&2
  exit 1
fi

if ! git -C "$REPO_ROOT" rev-parse --verify "$BASE" >/dev/null 2>&1; then
  echo "✗ Base branch not found: $BASE" >&2
  exit 1
fi

mkdir -p "$WT_ROOT"

echo "→ Creating worktree at $WT_PATH (branch: $BRANCH, base: $BASE)"
git -C "$REPO_ROOT" worktree add "$WT_PATH" -b "$BRANCH" "$BASE"

# Install frontend dependencies
DESKTOP_DIR="$WT_PATH/code/apps/desktop"
if [[ -f "$DESKTOP_DIR/package.json" ]]; then
  echo "→ Installing frontend dependencies"
  (
    cd "$DESKTOP_DIR"
    if command -v pnpm >/dev/null 2>&1; then
      pnpm install --frozen-lockfile 2>/dev/null || pnpm install
    else
      npm ci 2>/dev/null || npm install
    fi
  )
fi

# Rust: sccache is optional but recommended for faster incremental builds
if ! command -v sccache >/dev/null 2>&1; then
  echo "⚠ sccache not found. Install with: cargo install sccache"
  echo "  sccache caches compiled dependencies across worktrees."
fi

echo ""
echo "✓ Worktree ready: $WT_PATH"
echo "  cd $WT_PATH"
echo "  # or open in your editor:"
echo "  code $WT_PATH"
