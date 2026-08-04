#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="d-token-desktop"
APP_BUNDLE=""

find_bundle() {
  local candidates=(
    "${CARGO_TARGET_DIR:-}/release/bundle/macos/d-token.app"
    "$ROOT_DIR/code/target/release/bundle/macos/d-token.app"
    "$HOME/Library/Caches/d-token/tauri-target/release/bundle/macos/d-token.app"
  )
  local newest_mtime=0
  for candidate in "${candidates[@]}"; do
    if [[ -n "$candidate" && -d "$candidate" ]]; then
      local mtime
      mtime="$(stat -f '%m' "$candidate")"
      if (( mtime > newest_mtime )); then
        newest_mtime="$mtime"
        APP_BUNDLE="$candidate"
      fi
    fi
  done
  [[ -n "$APP_BUNDLE" ]]
}

build_app() {
  pkill -x "$APP_NAME" >/dev/null 2>&1 || true
  (cd "$ROOT_DIR/code/apps/desktop" && npm run tauri:build)
  find_bundle || {
    echo "Built d-token.app bundle could not be located" >&2
    exit 1
  }
}

open_app() {
  /usr/bin/open -n "$APP_BUNDLE"
}

case "$MODE" in
  run)
    build_app
    open_app
    ;;
  --verify|verify)
    build_app
    open_app
    sleep 1
    pgrep -x "$APP_NAME" >/dev/null
    echo "Verified $APP_NAME at $APP_BUNDLE"
    ;;
  --debug|debug)
    build_app
    lldb -- "$APP_BUNDLE/Contents/MacOS/$APP_NAME"
    ;;
  --logs|logs)
    build_app
    open_app
    /usr/bin/log stream --info --style compact --predicate "process == \"$APP_NAME\""
    ;;
  --telemetry|telemetry)
    build_app
    open_app
    /usr/bin/log stream --info --style compact --predicate 'subsystem == "ai.dtoken.control-plane"'
    ;;
  *)
    echo "usage: $0 [run|--verify|--debug|--logs|--telemetry]" >&2
    exit 2
    ;;
esac
