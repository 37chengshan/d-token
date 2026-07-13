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
  cargo build -p dtoken-proxy
)

SOURCE="$TARGET_DIR/debug/dtoken-proxy"
DESTINATION="$BINARY_DIR/dtoken-proxy-$HOST_TRIPLE"
if [[ ! -f "$SOURCE" ]]; then
  echo "dtoken-proxy check artifact was not found at $SOURCE" >&2
  exit 1
fi

mkdir -p "$BINARY_DIR"
cp "$SOURCE" "$DESTINATION"
chmod 755 "$DESTINATION"

printf '{"ok":true,"mode":"prepare-desktop-sidecar-for-checks","target_triple":"%s","destination":"%s"}\n' \
  "$HOST_TRIPLE" "$DESTINATION"
