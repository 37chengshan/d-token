#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DMG_PATH=""
EXPECTED_VERSION=""
REPORT_PATH=""
REQUIRE_DEVELOPER_ID=0
REQUIRE_NOTARIZATION=0
SKIP_LAUNCH=0

usage() {
  cat <<'EOF'
Usage: run_macos_release_package_audit.sh [options]

Options:
  --dmg PATH                 DMG to audit. Defaults to the newest release DMG.
  --expected-version VALUE   Expected CFBundleShortVersionString.
  --report PATH              Markdown report output path.
  --require-developer-id     Fail unless the app has a non-ad-hoc signing authority and team ID.
  --require-notarization     Require stapler validation and Gatekeeper acceptance.
  --skip-launch              Skip temporary installed-app startup verification.
  -h, --help                 Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg)
      DMG_PATH="$2"
      shift 2
      ;;
    --expected-version)
      EXPECTED_VERSION="$2"
      shift 2
      ;;
    --report)
      REPORT_PATH="$2"
      shift 2
      ;;
    --require-developer-id)
      REQUIRE_DEVELOPER_ID=1
      shift
      ;;
    --require-notarization)
      REQUIRE_NOTARIZATION=1
      shift
      ;;
    --skip-launch)
      SKIP_LAUNCH=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS package audit must run on macOS." >&2
  exit 1
fi

if [[ -z "$EXPECTED_VERSION" ]]; then
  EXPECTED_VERSION="$(python3 - "$ROOT/code/apps/desktop/package.json" <<'PY'
import json
import pathlib
import sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["version"])
PY
)"
fi

if [[ -z "$DMG_PATH" ]]; then
  DMG_PATH="$(python3 - "$ROOT/code/target/release/bundle/dmg" <<'PY'
import pathlib
import sys
root = pathlib.Path(sys.argv[1])
items = sorted(root.glob("*.dmg"), key=lambda item: item.stat().st_mtime, reverse=True)
if not items:
    raise SystemExit(1)
print(items[0])
PY
)" || {
    echo "No release DMG found under code/target/release/bundle/dmg." >&2
    exit 1
  }
fi

if [[ ! -f "$DMG_PATH" ]]; then
  echo "DMG does not exist: $DMG_PATH" >&2
  exit 1
fi
DMG_PATH="$(cd "$(dirname "$DMG_PATH")" && pwd)/$(basename "$DMG_PATH")"

if [[ -z "$REPORT_PATH" ]]; then
  REPORT_PATH="$ROOT/code/target/release/macos-package-audit.md"
fi
mkdir -p "$(dirname "$REPORT_PATH")"
REPORT_PATH="$(cd "$(dirname "$REPORT_PATH")" && pwd)/$(basename "$REPORT_PATH")"

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/d-token-release-audit.XXXXXX")"
MOUNT_POINT="$TMP_ROOT/mount"
INSTALL_ROOT="$TMP_ROOT/Applications"
STATE_ROOT="$TMP_ROOT/state"
APP_PID=""
MOUNTED=0

cleanup() {
  local exit_code=$?
  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    kill -TERM "$APP_PID" 2>/dev/null || true
    for _ in {1..20}; do
      kill -0 "$APP_PID" 2>/dev/null || break
      sleep 0.25
    done
    kill -KILL "$APP_PID" 2>/dev/null || true
  fi
  if [[ "$MOUNTED" -eq 1 ]]; then
    hdiutil detach "$MOUNT_POINT" -quiet -force >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_ROOT"
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

mkdir -p "$MOUNT_POINT" "$INSTALL_ROOT" "$STATE_ROOT"
hdiutil attach "$DMG_PATH" -nobrowse -readonly -mountpoint "$MOUNT_POINT" -quiet
MOUNTED=1

SOURCE_APP="$(python3 - "$MOUNT_POINT" <<'PY'
import pathlib
import sys
root = pathlib.Path(sys.argv[1])
items = sorted(root.glob("*.app"))
if len(items) != 1:
    raise SystemExit(f"expected exactly one .app in DMG root, found {len(items)}")
print(items[0])
PY
)"
INSTALLED_APP="$INSTALL_ROOT/d-token.app"
ditto "$SOURCE_APP" "$INSTALLED_APP"

APP_BINARY="$INSTALLED_APP/Contents/MacOS/d-token-desktop"
PROXY_BINARY="$INSTALLED_APP/Contents/MacOS/dtoken-proxy"
BRIDGE_BINARY="$INSTALLED_APP/Contents/MacOS/dtoken-agent-bridge"
CODE_GRAPH_LAUNCHER="$INSTALLED_APP/Contents/MacOS/dtoken-codebase-memory-launcher"
LEGAL_LICENSE="$INSTALLED_APP/Contents/Resources/legal/LICENSE"
LEGAL_NOTICES="$INSTALLED_APP/Contents/Resources/legal/THIRD_PARTY_NOTICES.md"
UPSTREAM_LICENSE="$INSTALLED_APP/Contents/Resources/codebase-memory/LICENSE"
UPSTREAM_NOTICES="$INSTALLED_APP/Contents/Resources/codebase-memory/THIRD_PARTY_NOTICES.md"

REQUIRED_FILES=(
  "$APP_BINARY"
  "$PROXY_BINARY"
  "$BRIDGE_BINARY"
  "$CODE_GRAPH_LAUNCHER"
  "$LEGAL_LICENSE"
  "$LEGAL_NOTICES"
  "$UPSTREAM_LICENSE"
  "$UPSTREAM_NOTICES"
)
for required in "${REQUIRED_FILES[@]}"; do
  if [[ ! -f "$required" ]]; then
    echo "Required bundle file is missing: $required" >&2
    exit 1
  fi
done

if [[ -e "$INSTALLED_APP/Contents/MacOS/codebase-memory-mcp" ]] || \
   [[ -e "$INSTALLED_APP/Contents/MacOS/codebase-memory-mcp.exe" ]]; then
  echo "The large upstream codebase-memory-mcp executable must not be bundled." >&2
  exit 1
fi

python3 - "$INSTALLED_APP" <<'PY'
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
for path in root.rglob("*"):
    relative = path.relative_to(root).as_posix().casefold()
    name = path.name.casefold()
    if name in {".ds_store", "thumbs.db"}:
        raise SystemExit(f"forbidden OS metadata in bundle: {relative}")
    if name.endswith((".p12", ".pfx", ".pem", ".key")):
        raise SystemExit(f"forbidden signing or secret material in bundle: {relative}")
    if any(marker in relative for marker in (
        "/.codex/",
        "/.ai-bridge/",
        "test/artifacts",
        "support/artifacts",
    )):
        raise SystemExit(f"forbidden local/reference content in bundle: {relative}")
    if any(part.startswith("headroom-") and part[9:10].isdigit() for part in relative.split("/")):
        raise SystemExit(f"forbidden Headroom reference workspace in bundle: {relative}")
PY

PLIST="$INSTALLED_APP/Contents/Info.plist"
BUNDLE_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PLIST")"
BUNDLE_IDENTIFIER="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$PLIST")"
if [[ "$BUNDLE_VERSION" != "$EXPECTED_VERSION" ]]; then
  echo "Bundle version mismatch: expected $EXPECTED_VERSION, got $BUNDLE_VERSION" >&2
  exit 1
fi
if [[ "$BUNDLE_IDENTIFIER" != "ai.dtoken.control-plane" ]]; then
  echo "Unexpected bundle identifier: $BUNDLE_IDENTIFIER" >&2
  exit 1
fi

codesign --verify --deep --strict --verbose=2 "$INSTALLED_APP"
for executable in "$APP_BINARY" "$PROXY_BINARY" "$BRIDGE_BINARY" "$CODE_GRAPH_LAUNCHER"; do
  codesign --verify --strict --verbose=2 "$executable"
done

SIGNATURE_DETAILS="$(codesign -dv --verbose=4 "$INSTALLED_APP" 2>&1)"
TEAM_IDENTIFIER="$(printf '%s\n' "$SIGNATURE_DETAILS" | awk -F= '/^TeamIdentifier=/{print $2; exit}')"
SIGNING_AUTHORITY="$(printf '%s\n' "$SIGNATURE_DETAILS" | awk -F= '/^Authority=/{print $2; exit}')"
SIGNATURE_KIND="developer-id"
if [[ -z "$SIGNING_AUTHORITY" || -z "$TEAM_IDENTIFIER" || "$TEAM_IDENTIFIER" == "not set" ]]; then
  SIGNATURE_KIND="ad-hoc"
fi
if [[ "$REQUIRE_DEVELOPER_ID" -eq 1 && "$SIGNATURE_KIND" != "developer-id" ]]; then
  echo "Developer ID signature is required; candidate is $SIGNATURE_KIND signed." >&2
  exit 1
fi

NOTARIZATION_STATUS="not-required"
GATEKEEPER_STATUS="not-required"
if [[ "$REQUIRE_NOTARIZATION" -eq 1 ]]; then
  xcrun stapler validate "$DMG_PATH"
  NOTARIZATION_STATUS="validated"
  spctl --assess --type execute --verbose=2 "$INSTALLED_APP"
  GATEKEEPER_STATUS="accepted"
fi

LAUNCH_STATUS="skipped"
if [[ "$SKIP_LAUNCH" -eq 0 ]]; then
  DTOKEN_TEST_STATE_DIR="$STATE_ROOT" "$APP_BINARY" >"$TMP_ROOT/app.stdout" 2>"$TMP_ROOT/app.stderr" &
  APP_PID=$!
  STARTED=0
  for _ in {1..40}; do
    if kill -0 "$APP_PID" 2>/dev/null; then
      STARTED=1
      sleep 0.25
      continue
    fi
    break
  done
  if [[ "$STARTED" -ne 1 ]] || ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "Installed application did not remain running during startup audit." >&2
    if [[ -s "$TMP_ROOT/app.stderr" ]]; then
      tail -n 40 "$TMP_ROOT/app.stderr" >&2
    fi
    exit 1
  fi
  LAUNCH_STATUS="started"
  kill -TERM "$APP_PID"
  for _ in {1..40}; do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.25
  done
  if kill -0 "$APP_PID" 2>/dev/null; then
    kill -KILL "$APP_PID" 2>/dev/null || true
  fi
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
fi

DMG_SHA256="$(shasum -a 256 "$DMG_PATH" | awk '{print $1}')"
APP_BINARY_SHA256="$(shasum -a 256 "$APP_BINARY" | awk '{print $1}')"
FILE_COUNT="$(python3 - "$INSTALLED_APP" <<'PY'
import pathlib
import sys
print(sum(1 for path in pathlib.Path(sys.argv[1]).rglob("*") if path.is_file()))
PY
)"
APP_SIZE_BYTES="$(du -sk "$INSTALLED_APP" | awk '{print $1 * 1024}')"
DMG_SIZE_BYTES="$(stat -f '%z' "$DMG_PATH")"

rm -rf "$INSTALLED_APP"
if [[ -e "$INSTALLED_APP" ]]; then
  echo "Temporary uninstall failed: $INSTALLED_APP still exists" >&2
  exit 1
fi
UNINSTALL_STATUS="removed-temporary-install"

cat > "$REPORT_PATH" <<EOF
# macOS Package Audit

- Candidate: \`d-token $EXPECTED_VERSION\`
- DMG: \`$(basename "$DMG_PATH")\`
- DMG SHA-256: \`$DMG_SHA256\`
- DMG size: \`$DMG_SIZE_BYTES\` bytes
- Bundle identifier: \`$BUNDLE_IDENTIFIER\`
- Bundle version: \`$BUNDLE_VERSION\`
- Main binary SHA-256: \`$APP_BINARY_SHA256\`
- App size: \`$APP_SIZE_BYTES\` bytes
- Bundle file count: \`$FILE_COUNT\`
- Signature: \`$SIGNATURE_KIND\`
- Team identifier: \`${TEAM_IDENTIFIER:-not-set}\`
- Signing authority: \`${SIGNING_AUTHORITY:-not-set}\`
- Notarization: \`$NOTARIZATION_STATUS\`
- Gatekeeper: \`$GATEKEEPER_STATUS\`
- Temporary install/start: \`$LAUNCH_STATUS\`
- Temporary uninstall: \`$UNINSTALL_STATUS\`

## Required bundle content

- d-token desktop executable
- d-token proxy runtime
- Agent bridge
- restricted Code Graph launcher
- project Apache-2.0 license
- distribution third-party notices
- Codebase Memory upstream legal text

## Enforced exclusions

- no \`codebase-memory-mcp\` upstream executable
- no versioned \`headroom-*/\` reference workspace
- no local \`.codex/\`, \`.ai-bridge/\` or test/support artifacts
- no signing certificate or private-key material
- no OS metadata files
EOF

printf 'macOS package audit passed: %s (%s, %s)\n' \
  "$(basename "$DMG_PATH")" "$SIGNATURE_KIND" "$LAUNCH_STATUS"
printf 'Audit report: %s\n' "$REPORT_PATH"
