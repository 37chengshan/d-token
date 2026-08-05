#!/usr/bin/env bash
set -euo pipefail

umask 077

die() {
  echo "error: $*" >&2
  exit 1
}

[[ "$(uname -s)" == "Darwin" ]] || die "this candidate test requires macOS"
for command in cargo python3; do
  command -v "$command" >/dev/null 2>&1 || die "$command is required"
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CODE_ROOT="$ROOT/code"
ACTIVE_CONFIG="${HOME}/Library/Application Support/d-token/active-config/dtoken-config.json"
[[ -f "$ACTIVE_CONFIG" && ! -L "$ACTIVE_CONFIG" ]] || die "a regular active d-token config is required"

HOST_CONFIG_FINGERPRINT="$(python3 - "$ACTIVE_CONFIG" <<'PY'
import hashlib
import pathlib
import sys
print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
MANAGED_ROOT="${HOME}/Library/Application Support/d-token/candidate-rtk/${STAMP}-$$"
REPORT_DIR="$ROOT/support/artifacts/managed-rtk-candidate"
REPORT_PATH="$REPORT_DIR/candidate-${STAMP}-$$.json"
mkdir -p "$REPORT_DIR"
chmod 700 "$REPORT_DIR"

(
  cd "$CODE_ROOT"
  cargo build -p dtoken-agent-bridge
  cargo build -p dtoken-agent-bridge --example managed_rtk_candidate_probe
)

RECEIPT="$(
  "$CODE_ROOT/target/debug/examples/managed_rtk_candidate_probe" \
    "$CODE_ROOT/target/debug/dtoken-agent-bridge" \
    "$MANAGED_ROOT" \
    "$HOST_CONFIG_FINGERPRINT"
)"

python3 - "$REPORT_PATH" "$RECEIPT" <<'PY'
import json
import os
import pathlib
import sys
import tempfile
from datetime import datetime, timezone

report_path, receipt = sys.argv[1:]
payload = {
    "schema_version": 1,
    "created_at": datetime.now(timezone.utc).isoformat(),
    "status": "pass",
    "candidate_id": os.environ.get("DTOKEN_RELEASE_CANDIDATE_ID") or None,
    **json.loads(receipt),
    "active_config_fingerprint_bound": True,
    "managed_root": "<d-token-private-candidate-rtk>",
    "report_path": "support/artifacts/managed-rtk-candidate/" + pathlib.Path(report_path).name,
}
destination = pathlib.Path(report_path)
fd, temporary = tempfile.mkstemp(prefix=destination.name + ".", dir=destination.parent)
try:
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, separators=(",", ":"))
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, destination)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
PY
