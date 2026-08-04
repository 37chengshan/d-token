#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
BINARY_DIR="$CODE_ROOT/apps/desktop/src-tauri/binaries"
TARGET_DIR="${CARGO_TARGET_DIR:-$CODE_ROOT/target}"
HOST_TRIPLE="$(rustc -vV | awk '/^host:/ { print $2 }')"

if [[ -z "$HOST_TRIPLE" ]]; then
  echo "Could not determine the Rust host target triple." >&2
  exit 1
fi

case "$HOST_TRIPLE" in
  *-unknown-linux-gnu|*-unknown-linux-musl)
    ;;
  *)
    echo "prepare_desktop_sidecar_for_checks.sh only supports Linux CI hosts; got $HOST_TRIPLE" >&2
    exit 1
    ;;
esac

(
  cd "$CODE_ROOT"
  cargo build -p dtoken-proxy -p dtoken-agent-bridge -p dtoken-codebase-memory
)

SOURCE="$TARGET_DIR/debug/dtoken-proxy"
DESTINATION="$BINARY_DIR/dtoken-proxy-$HOST_TRIPLE"
BRIDGE_SOURCE="$TARGET_DIR/debug/dtoken-agent-bridge"
BRIDGE_DESTINATION="$BINARY_DIR/dtoken-agent-bridge-$HOST_TRIPLE"
LAUNCHER_SOURCE="$TARGET_DIR/debug/dtoken-codebase-memory-launcher"
LAUNCHER_DESTINATION="$BINARY_DIR/dtoken-codebase-memory-launcher-$HOST_TRIPLE"
if [[ ! -f "$SOURCE" ]]; then
  echo "dtoken-proxy check artifact was not found at $SOURCE" >&2
  exit 1
fi
if [[ ! -f "$BRIDGE_SOURCE" ]]; then
  echo "dtoken-agent-bridge check artifact was not found at $BRIDGE_SOURCE" >&2
  exit 1
fi
if [[ ! -f "$LAUNCHER_SOURCE" ]]; then
  echo "dtoken-codebase-memory-launcher check artifact was not found at $LAUNCHER_SOURCE" >&2
  exit 1
fi

mkdir -p "$BINARY_DIR"
cp "$SOURCE" "$DESTINATION"
chmod 755 "$DESTINATION"
cp "$BRIDGE_SOURCE" "$BRIDGE_DESTINATION"
chmod 755 "$BRIDGE_DESTINATION"
cp "$LAUNCHER_SOURCE" "$LAUNCHER_DESTINATION"
chmod 755 "$LAUNCHER_DESTINATION"
mkdir -p "$BINARY_DIR/codebase-memory"

printf '{"ok":true,"mode":"prepare-desktop-sidecar-for-checks","target_triple":"%s","destination":"%s","agent_bridge":"%s","codebase_memory_launcher":"%s","codebase_memory_distribution":"user-imported-pinned-upstream"}\n' \
  "$HOST_TRIPLE" "$DESTINATION" "$BRIDGE_DESTINATION" "$LAUNCHER_DESTINATION"
