#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
BINARY_DIR="$CODE_ROOT/apps/desktop/src-tauri/binaries"
PROFILE="debug"
REQUESTED_TARGET=""
DRY_RUN=0

usage() {
  cat <<'EOF'
Usage: prepare_desktop_sidecars_macos.sh [--release] [--target <target>] [--dry-run]

Targets:
  aarch64-apple-darwin
  x86_64-apple-darwin
  universal-apple-darwin

When --target is omitted, the current macOS host architecture is used.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release|-Release)
      PROFILE="release"
      shift
      ;;
    --debug)
      PROFILE="debug"
      shift
      ;;
    --target)
      if [[ $# -lt 2 ]]; then
        echo "--target requires a value" >&2
        exit 2
      fi
      REQUESTED_TARGET="$2"
      shift 2
      ;;
    --target=*)
      REQUESTED_TARGET="${1#--target=}"
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "prepare_desktop_sidecars_macos.sh must run on macOS/Darwin" >&2
  exit 1
fi

if [[ -z "$REQUESTED_TARGET" ]]; then
  REQUESTED_TARGET="$(uname -m)"
fi

case "$REQUESTED_TARGET" in
  arm64|aarch64|aarch64-apple-darwin)
    TARGET_TRIPLE="aarch64-apple-darwin"
    ;;
  x86_64|x64|x86_64-apple-darwin)
    TARGET_TRIPLE="x86_64-apple-darwin"
    ;;
  universal|universal-apple-darwin)
    TARGET_TRIPLE="universal-apple-darwin"
    ;;
  *)
    echo "unsupported macOS target: $REQUESTED_TARGET" >&2
    exit 1
    ;;
esac

source_path_for_target() {
  printf '%s/target/%s/%s/dtoken-proxy' "$CODE_ROOT" "$1" "$PROFILE"
}

destination_path_for_target() {
  printf '%s/dtoken-proxy-%s' "$BINARY_DIR" "$1"
}

ensure_rust_target() {
  local target="$1"
  if command -v rustup >/dev/null 2>&1 && ! rustup target list --installed | grep -Fxq "$target"; then
    echo "Rust target $target is not installed; run: rustup target add $target" >&2
    exit 1
  fi
}

build_target() {
  local target="$1"
  local cargo_args=(build -p dtoken-proxy --target "$target" --target-dir "$CODE_ROOT/target")
  if [[ "$PROFILE" == "release" ]]; then
    cargo_args+=(--release)
  fi

  ensure_rust_target "$target"
  pushd "$CODE_ROOT" >/dev/null
  cargo "${cargo_args[@]}"
  popd >/dev/null
}

copy_sidecar() {
  local source="$1"
  local target="$2"
  local destination
  local temporary
  destination="$(destination_path_for_target "$target")"
  temporary="$destination.tmp.$$"

  if [[ ! -f "$source" ]]; then
    echo "dtoken-proxy build artifact not found at $source" >&2
    exit 1
  fi

  mkdir -p "$BINARY_DIR"
  cp "$source" "$temporary"
  chmod +x "$temporary"
  mv -f "$temporary" "$destination"
  printf '%s' "$destination"
}

if [[ "$DRY_RUN" -eq 1 ]]; then
  printf 'profile=%s target=%s\n' "$PROFILE" "$TARGET_TRIPLE"
  if [[ "$TARGET_TRIPLE" == "universal-apple-darwin" ]]; then
    printf 'source_aarch64=%s\n' "$(source_path_for_target aarch64-apple-darwin)"
    printf 'source_x86_64=%s\n' "$(source_path_for_target x86_64-apple-darwin)"
  else
    printf 'source=%s\n' "$(source_path_for_target "$TARGET_TRIPLE")"
  fi
  printf 'destination=%s\n' "$(destination_path_for_target "$TARGET_TRIPLE")"
  exit 0
fi

if [[ "$TARGET_TRIPLE" == "universal-apple-darwin" ]]; then
  if ! command -v lipo >/dev/null 2>&1; then
    echo "lipo is required for universal-apple-darwin; install Xcode Command Line Tools" >&2
    exit 1
  fi
  ARM_TARGET="aarch64-apple-darwin"
  INTEL_TARGET="x86_64-apple-darwin"
  build_target "$ARM_TARGET"
  build_target "$INTEL_TARGET"
  ARM_SOURCE="$(source_path_for_target "$ARM_TARGET")"
  INTEL_SOURCE="$(source_path_for_target "$INTEL_TARGET")"
  UNIVERSAL_SOURCE="$CODE_ROOT/target/$TARGET_TRIPLE/$PROFILE/dtoken-proxy"
  mkdir -p "$(dirname "$UNIVERSAL_SOURCE")"
  lipo -create "$ARM_SOURCE" "$INTEL_SOURCE" -output "$UNIVERSAL_SOURCE"
  SOURCE="$UNIVERSAL_SOURCE"
else
  build_target "$TARGET_TRIPLE"
  SOURCE="$(source_path_for_target "$TARGET_TRIPLE")"
fi

DEST="$(copy_sidecar "$SOURCE" "$TARGET_TRIPLE")"

node --input-type=module - "$SOURCE" "$DEST" "$PROFILE" "$TARGET_TRIPLE" <<'JS'
import { createHash } from "node:crypto";
import { readFileSync, statSync } from "node:fs";
import process from "node:process";

const [source, destination, profile, target] = process.argv.slice(2);
const bytes = readFileSync(destination);

process.stdout.write(`${JSON.stringify({
  ok: true,
  mode: "prepare-desktop-sidecars-macos",
  profile,
  target_triple: target,
  source,
  destination,
  bytes: statSync(destination).size,
  sha256: createHash("sha256").update(bytes).digest("hex"),
})}\n`);
JS
