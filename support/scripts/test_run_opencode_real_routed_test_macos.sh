#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/support/scripts/run_opencode_real_routed_test_macos.sh"
TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/dtoken-opencode-dryrun-test.XXXXXX")"

cleanup() {
  rm -rf "$TEMP_ROOT"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ "$(uname -s)" == "Darwin" ]] || fail "this focused test requires macOS"
[[ -x "$SCRIPT" ]] || fail "real routed test script must be executable"
bash -n "$SCRIPT"

mkdir -p "$TEMP_ROOT/bin"
python3 - "$TEMP_ROOT/bin/opencode" "$TEMP_ROOT/opencode-called" <<'PY'
import os
import pathlib
import sys

executable, marker = map(pathlib.Path, sys.argv[1:])
executable.write_text(f"#!/bin/sh\ntouch '{marker}'\nexit 99\n", encoding="utf-8")
os.chmod(executable, 0o700)
PY

CONFIG_PATH="$TEMP_ROOT/opencode.json"
python3 - "$CONFIG_PATH" <<'PY'
import json
import pathlib
import sys

pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "provider": {"existing": {"name": "Existing provider"}},
    "enabled_providers": ["existing"],
}, separators=(",", ":")) + "\n", encoding="utf-8")
PY

PORT="$(python3 - <<'PY'
import socket
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
    listener.bind(("127.0.0.1", 0))
    print(listener.getsockname()[1])
PY
)"
BEFORE_HASH="$(shasum -a 256 "$CONFIG_PATH" | awk '{print $1}')"
PATH="$TEMP_ROOT/bin:$PATH" "$SCRIPT" \
  --dry-run \
  --config-path "$CONFIG_PATH" \
  --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 \
  --api-key-ref keychain://dtoken-test/existing \
  --port "$PORT" \
  >"$TEMP_ROOT/result.json"
AFTER_HASH="$(shasum -a 256 "$CONFIG_PATH" | awk '{print $1}')"

[[ "$BEFORE_HASH" == "$AFTER_HASH" ]] || fail "dry-run changed the OpenCode config"
[[ ! -e "$TEMP_ROOT/opencode-called" ]] || fail "dry-run executed OpenCode"
[[ "$(find "$TEMP_ROOT" -maxdepth 1 -name '*.dtoken-backup-*' | wc -l | tr -d ' ')" == "0" ]] || fail "dry-run created a backup"
python3 - "$TEMP_ROOT/result.json" <<'PY'
import json
import pathlib
import sys

result = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert result["status"] == "preflight_ok"
assert result["mode"] == "dry_run"
assert result["credential_mode"] == "os_store_reference"
assert result["strict_json_valid"] is True
assert result["config_mutated"] is False
assert result["backup_created"] is False
assert result["report_created"] is False
assert result["opencode_executed"] is False
assert result["external_request_made"] is False
assert result["normal_user_config"].startswith("<explicit-config>/")
PY

INVALID_PATH="$TEMP_ROOT/invalid.json"
python3 - "$INVALID_PATH" <<'PY'
import pathlib
import sys
pathlib.Path(sys.argv[1]).write_text('{"provider":{},}\n', encoding="utf-8")
PY
INVALID_HASH="$(shasum -a 256 "$INVALID_PATH" | awk '{print $1}')"
if "$SCRIPT" --dry-run --config-path "$INVALID_PATH" --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 --upstream-no-auth --port "$PORT" \
  >"$TEMP_ROOT/invalid.out" 2>"$TEMP_ROOT/invalid.err"; then
  fail "invalid JSON unexpectedly passed"
fi
[[ "$INVALID_HASH" == "$(shasum -a 256 "$INVALID_PATH" | awk '{print $1}')" ]] || fail "failed preflight changed invalid config"

CONFLICT_PATH="$TEMP_ROOT/conflict.json"
python3 - "$CONFLICT_PATH" <<'PY'
import json
import pathlib
import sys
pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "provider": {"dtoken-local-route": {}},
}) + "\n", encoding="utf-8")
PY
if "$SCRIPT" --dry-run --config-path "$CONFLICT_PATH" --model existing/model-v1 \
  --upstream-base-url https://example.invalid/v1 --upstream-no-auth --port "$PORT" \
  >"$TEMP_ROOT/conflict.out" 2>"$TEMP_ROOT/conflict.err"; then
  fail "existing dtoken-local-route provider unexpectedly passed"
fi

rg -q "strict UTF-8 JSON" "$TEMP_ROOT/invalid.err" || fail "invalid JSON did not fail closed clearly"
rg -q "already exists" "$TEMP_ROOT/conflict.err" || fail "provider conflict did not fail closed clearly"
echo "PASS: OpenCode real routed dry-run is read-only and fail-closed"
