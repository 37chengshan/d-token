#!/usr/bin/env bash
set -euo pipefail

PROFILE="debug"
if [[ "${1:-}" == "--release" || "${1:-}" == "-Release" ]]; then
  PROFILE="release"
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
BINARY_DIR="$ROOT/code/apps/desktop/src-tauri/binaries"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "prepare_desktop_sidecars_macos.sh must run on macOS/Darwin" >&2
  exit 1
fi

HOST_ARCH="$(uname -m)"
case "$HOST_ARCH" in
  arm64)
    TARGET_TRIPLE="aarch64-apple-darwin"
    ;;
  x86_64)
    TARGET_TRIPLE="x86_64-apple-darwin"
    ;;
  *)
    echo "unsupported macOS architecture: $HOST_ARCH" >&2
    exit 1
    ;;
esac

pushd "$CODE_ROOT" >/dev/null
if [[ "$PROFILE" == "release" ]]; then
  cargo build -p dtoken-proxy --release
else
  cargo build -p dtoken-proxy
fi
popd >/dev/null

SOURCE="$CODE_ROOT/target/$PROFILE/dtoken-proxy"
if [[ ! -f "$SOURCE" ]]; then
  echo "dtoken-proxy build artifact not found at $SOURCE" >&2
  exit 1
fi

mkdir -p "$BINARY_DIR"
DEST="$BINARY_DIR/dtoken-proxy-$TARGET_TRIPLE"
cp "$SOURCE" "$DEST"
chmod +x "$DEST"

python3 - "$SOURCE" "$DEST" "$PROFILE" "$TARGET_TRIPLE" <<'PY'
import hashlib
import json
import os
import sys

source, dest, profile, target = sys.argv[1:]

def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

print(json.dumps({
    "ok": True,
    "mode": "prepare-desktop-sidecars-macos",
    "profile": profile,
    "target_triple": target,
    "source": source,
    "destination": dest,
    "bytes": os.path.getsize(dest),
    "sha256": sha256(dest),
}, separators=(",", ":")))
PY
