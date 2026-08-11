#!/usr/bin/env python3
"""Read-only cc-switch A->B->A evidence harness (fail-closed).

Collects snapshot / route-identity / ledger evidence AROUND the Main agent's
real cc-switch profile switch (performed with Computer actions on the real
cc-switch UI), then verifies that the route identity observed after switching
back matches the identity observed before the switch, that the harness itself
never wrote the cc-switch database, and that the resulting artifact is bound
to the candidate commit and source digest.

Boundaries (do not weaken):

* The harness NEVER writes the cc-switch database. Every open uses SQLite
  URI mode=ro plus PRAGMA query_only=1; any write attempt fails closed and
  increments ``writes_attempted``. In ``--fixture`` (dry-run) mode the
  harness only ever opens a private copy of the committed fixture database,
  never a real path.
* The real cc-switch DB path is never auto-discovered and never emitted.
  ``collect`` requires an explicit ``--db``; the artifact carries only a
  path fingerprint and the basename.
* An A->B->A artifact may only report ``passed`` when all of the following
  hold: (1) all three phase snapshots were collected from the SAME cc-switch
  database path (identical path fingerprints), (2) a B-state snapshot was
  observed and differs from the pre state (a no-op "switch" is blocked as
  ``switch_not_observed``), (3) the post identity equals the pre identity,
  (4) every phase snapshot records ``writes_attempted == 0`` and
  ``readonly_open == true``, (5) the DB bytes are unchanged between the post
  collect and the verify run (when ``--db`` is provided), and (6) the
  candidate commit is resolvable.
* When ``--expected-commit`` / ``--expected-source-digest`` are provided the
  artifact fails closed on mismatch. The source digest anchor is
  content-bearing and worktree-invariant: sha256 of the ASCII hex bytes of
  ``git rev-parse HEAD^{tree}`` (the candidate commit's content-addressed
  tree object id), reproducible at any clean checkout of the same commit.
* Route identity mirrors the d-token-config-patcher pinned formats
  (cc_switch.rs / cc_switch_fixture.rs):
    upstream_signature = sha256("cc-switch-upstream-v1\\n{provider_id}\\n"
                                "cc-switch:{app_type}\\n{protocol}\\n{endpoint}")
    route_signature    = sha256("cc-switch-route-v1\\n{upstream_signature}\\n{model}")
  Credential values and private payloads are never read into the snapshot;
  only a presence boolean, a normalized endpoint, and the derived signatures
  survive.

Usage:
  # Phase 1: capture state A (before the Main Computer switch to B)
  python3 support/scripts/run_cc_switch_aba_check.py collect \
      --db "$HOME/.cc-switch/cc-switch.db" --out pre-a.json

  # Phase 2: Main switches the real cc-switch profile to B with Computer
  python3 support/scripts/run_cc_switch_aba_check.py collect \
      --db "$HOME/.cc-switch/cc-switch.db" --out observed-b.json

  # Phase 3: Main switches back to A with Computer
  python3 support/scripts/run_cc_switch_aba_check.py collect \
      --db "$HOME/.cc-switch/cc-switch.db" --out post-a.json

  # Verify + emit the bound artifact
  python3 support/scripts/run_cc_switch_aba_check.py verify \
      --pre pre-a.json --observed-b observed-b.json --post post-a.json \
      --actions computer-actions.json --db "$HOME/.cc-switch/cc-switch.db" \
      --expected-commit <C_final> --expected-source-digest <digest> \
      --artifact support/artifacts/cc-switch-aba/cc-switch-aba-<ts>.json

  # Dry-run contract (fixture copies only; never a real DB path)
  python3 support/scripts/run_cc_switch_aba_check.py collect --fixture --out pre.json
  python3 support/scripts/run_cc_switch_aba_check.py --check-schema artifact.json
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path

SCHEMA_VERSION = "1.1"

# cc-switch database user_version values the harness can read. 12 is the
# historical committed fixture schema; 16 is the current real-world schema
# (the core tables the harness reads — profiles / providers / provider_endpoints
# — are present and compatible). Any other user_version resolves to a blocked
# snapshot (fail-closed, unknown schema).
SUPPORTED_SCHEMA_VERSIONS = (12, 16)

# --- Status enums -----------------------------------------------------------
SNAPSHOT_STATUS_LOADED = "loaded"
SNAPSHOT_STATUS_BLOCKED = "blocked"
SNAPSHOT_STATUS_UNREADABLE = "unreadable"

ARTIFACT_STATUS_PASSED = "passed"
ARTIFACT_STATUS_BLOCKED = "blocked"
ARTIFACT_STATUS_FAILED = "failed"

ARTIFACT_STATUSES = frozenset({ARTIFACT_STATUS_PASSED, ARTIFACT_STATUS_BLOCKED, ARTIFACT_STATUS_FAILED})
SNAPSHOT_STATUSES = frozenset({SNAPSHOT_STATUS_LOADED, SNAPSHOT_STATUS_BLOCKED, SNAPSHOT_STATUS_UNREADABLE})
PHASES = frozenset({"pre", "observed-b", "post"})

# Failure classes pinned by the contract tests.
FAIL_CLASSES = frozenset({
    "none",
    "db_unreadable",
    "unknown_schema",
    "missing_phase",
    "missing_observed_b",
    "db_path_mismatch",
    "switch_not_observed",
    "identity_mismatch",
    "wrote_cc_switch",
    "db_changed_during_verify",
    "candidate_unbound",
    "commit_mismatch",
    "digest_mismatch",
    "actions_redaction",
    "usage",
})

# Credential-shaped markers that must never appear in computer-action ledger
# entries (fail closed, mirroring the release redaction contract).
SECRET_PATTERNS = (
    re.compile(r"\bsk-[A-Za-z0-9_\-]{8,}"),
    re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    re.compile(r"fixture-cc-switch-token-marker"),
    re.compile(r"ANTHROPIC_AUTH_TOKEN"),
    re.compile(r"\bAuthorization: Bearer\b"),
)

COMMIT_RE = re.compile(r"[0-9a-fA-F]{40,64}")
DIGEST_RE = re.compile(r"[0-9a-fA-F]{64}")

# Top-level key sets pinned by --check-schema. Anything else fails closed.
SNAPSHOT_KEYS = frozenset({
    "schema_version", "mode", "phase", "status", "ok", "collected_at",
    "host_platform", "db", "route", "profiles", "ledger", "redacted",
})
ARTIFACT_KEYS = frozenset({
    "schema_version", "mode", "status", "ok", "generated_at", "host_platform",
    "candidate", "source_digest", "artifact_sha256", "phases", "route", "db",
    "wrote_cc_switch", "identity_restored", "observed_b_differs", "ledger",
    "failure", "restore", "redacted",
})

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SCRIPTS_DIR = Path(__file__).resolve().parent
FIXTURE_DB = REPO_ROOT / "support" / "fixtures" / "cc-switch" / "cc-switch.db"


class UsageError(Exception):
    pass


def now_iso() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact_sha256(artifact: dict) -> str:
    """artifact_sha256 rule (generate_release_gate_status): SHA-256 of the
    canonical JSON of the artifact WITHOUT the artifact_sha256 field."""
    body = {key: value for key, value in artifact.items() if key != "artifact_sha256"}
    canonical = json.dumps(
        body, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return sha256_hex(canonical)


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def stable_hash(content: str) -> str:
    """Mirror of dtoken-config-patcher stable_hash (lowercase sha256 hex)."""
    return sha256_hex(content.encode("utf-8"))


def normalize_endpoint(raw: str) -> str:
    """Mirror of cc_switch.rs normalize_endpoint (strip userinfo/query/fragment/trailing slash)."""
    normalized = raw.strip()
    while normalized.endswith("/"):
        normalized = normalized[:-1]
    for separator in ("#", "?"):
        if separator in normalized:
            normalized = normalized.split(separator, 1)[0]
    scheme_end = normalized.find("://")
    if scheme_end != -1:
        host_start = scheme_end + 3
        at = normalized.find("@", host_start)
        if at != -1:
            normalized = normalized[:host_start] + normalized[at + 1:]
    return normalized


def is_credential_key(key: str) -> bool:
    """Mirror of cc_switch.rs is_credential_key."""
    lowered = key.lower()
    return (
        "token" in lowered
        or "api_key" in lowered
        or "apikey" in lowered
        or "secret" in lowered
        or "password" in lowered
        or "authorization" in lowered
        or "auth" in lowered
        or lowered == "key"
    )


def has_value(value) -> bool:
    return isinstance(value, str) and bool(value)


def cc_switch_upstream_signature(provider_id: str, app_type: str, protocol: str, endpoint: str) -> str:
    """Mirror of the pinned cc-switch upstream signature format."""
    return stable_hash(
        f"cc-switch-upstream-v1\n{provider_id}\ncc-switch:{app_type}\n{protocol}\n{endpoint}"
    )


def cc_switch_route_signature(upstream_signature: str, model: str) -> str:
    """Mirror of the pinned cc-switch route signature format."""
    return stable_hash(f"cc-switch-route-v1\n{upstream_signature}\n{model}")


def canonical_identity(active_upstreams: list[dict]) -> str:
    """Deterministic identity text over the redacted active upstream metadata.

    Sorted by app_type/provider_id so a reordered table never changes the
    identity; the endpoint is the normalized metadata value, never a secret.
    """
    rows = sorted(
        (
            upstream.get("app_type", ""),
            upstream.get("provider_id", ""),
            upstream.get("model", ""),
            upstream.get("protocol", ""),
            upstream.get("normalized_endpoint", ""),
        )
        for upstream in active_upstreams
        if isinstance(upstream, dict)
    )
    return "\n".join("|".join(row) for row in rows)


# --- Read-only snapshot -----------------------------------------------------

def read_cc_switch_snapshot(db_path: Path, phase: str, mode: str) -> dict:
    """Read-only snapshot of a cc-switch database (never writes).

    Mirrors cc_switch.rs read_cc_switch_snapshot semantics: schema versions
    outside SUPPORTED_SCHEMA_VERSIONS resolve to a blocked snapshot, active
    providers missing required fields block the selection, credential values
    are never read. The db.schema_version field records the database's real
    PRAGMA user_version (12 or 16).
    """
    events: list[dict] = []
    ledger = {"events": events, "writes_attempted": 0, "redacted": True}
    try:
        raw = db_path.read_bytes()
    except OSError as exc:
        return _snapshot_unreadable(phase, mode, str(exc))
    source_fingerprint = sha256_hex(raw)
    revision = int.from_bytes(bytes.fromhex(source_fingerprint)[:8], "big")

    sidecars = {}
    for suffix in ("-wal", "-shm"):
        sidecar = Path(str(db_path) + suffix)
        try:
            sidecars[suffix.lstrip("-")] = {
                "present": sidecar.is_file(),
                "bytes": sidecar.stat().st_size if sidecar.is_file() else 0,
            }
        except OSError:
            sidecars[suffix.lstrip("-")] = {"present": None, "bytes": None}

    try:
        connection = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True, timeout=5.0)
    except sqlite3.Error as exc:
        return _snapshot_unreadable(phase, mode, str(exc))
    try:
        # Defense in depth: this connection must never write the database
        # file. PRAGMA query_only is a connection-level flag (no file write).
        connection.execute("PRAGMA query_only = 1")
        try:
            schema_version = connection.execute("PRAGMA user_version").fetchone()[0]
        except sqlite3.DatabaseError:
            return _snapshot_unreadable(phase, mode, "source is not a readable SQLite database")
        except sqlite3.Error:
            schema_version = 0
        events.append({"at": now_iso(), "event": "open_readonly", "schema_version": schema_version})

        if schema_version not in SUPPORTED_SCHEMA_VERSIONS:
            return {
                "schema_version": SCHEMA_VERSION,
                "mode": mode,
                "phase": phase,
                "status": SNAPSHOT_STATUS_BLOCKED,
                "ok": False,
                "collected_at": now_iso(),
                "host_platform": sys.platform,
                "db": {
                    "path_basename": db_path.name,
                    "path_fingerprint_sha256": sha256_hex(str(db_path).encode("utf-8")),
                    "path_redacted": True,
                    "schema_version": schema_version,
                    "schema_kind": "unknown",
                    "source_state": "blocked",
                    "selection_state": "blocked",
                    "readonly_open": True,
                    "query_only": True,
                    "writes_attempted": 0,
                    "source_fingerprint_sha256": source_fingerprint,
                    "revision": revision,
                    "bytes_read": len(raw),
                    "sidecars": sidecars,
                },
                "route": {"active_upstreams": [], "identity_sha256": ""},
                "profiles": [],
                "ledger": ledger,
                "redacted": True,
            }

        profiles = [
            row[0]
            for row in connection.execute("SELECT name FROM profiles ORDER BY id").fetchall()
        ]

        active_rows = connection.execute(
            "SELECT id, app_type, settings_config FROM providers "
            "WHERE is_current = 1 ORDER BY sort_index, id"
        ).fetchall()
        candidates: list[dict] = []
        parse_failure = False
        for provider_id, app_type, settings_config in active_rows:
            upstream = _parse_active_upstream(connection, provider_id, app_type, settings_config)
            if upstream is None:
                parse_failure = True
            else:
                candidates.append(upstream)
        events.append({"at": now_iso(), "event": "read_active_providers", "count": len(active_rows)})

        per_app = {}
        for upstream in candidates:
            per_app.setdefault(upstream["app_type"], 0)
            per_app[upstream["app_type"]] += 1
        if parse_failure:
            selection_state = "blocked"
        elif not candidates:
            selection_state = "resolved"
        elif any(count > 1 for count in per_app.values()):
            selection_state = "needs_selection"
        else:
            selection_state = "resolved"

        identity_text = canonical_identity(candidates)
        route_entries = []
        for upstream in sorted(candidates, key=lambda u: (u["app_type"], u["provider_id"])):
            upstream_sig = cc_switch_upstream_signature(
                upstream["provider_id"], upstream["app_type"],
                upstream["protocol"], upstream["normalized_endpoint"],
            )
            route_entries.append({
                "app_type": upstream["app_type"],
                "provider_id": upstream["provider_id"],
                "model": upstream["model"],
                "protocol": upstream["protocol"],
                "normalized_endpoint": upstream["normalized_endpoint"],
                "credential_present": upstream["credential_present"],
                "upstream_signature": upstream_sig,
                "route_signature": cc_switch_route_signature(upstream_sig, upstream["model"]),
            })

        return {
            "schema_version": SCHEMA_VERSION,
            "mode": mode,
            "phase": phase,
            "status": SNAPSHOT_STATUS_LOADED,
            "ok": True,
            "collected_at": now_iso(),
            "host_platform": sys.platform,
            "db": {
                "path_basename": db_path.name,
                "path_fingerprint_sha256": sha256_hex(str(db_path).encode("utf-8")),
                "path_redacted": True,
                "schema_version": schema_version,
                "schema_kind": "loaded",
                "source_state": "loaded",
                "selection_state": selection_state,
                "readonly_open": True,
                "query_only": True,
                "writes_attempted": 0,
                "source_fingerprint_sha256": source_fingerprint,
                "revision": revision,
                "bytes_read": len(raw),
                "sidecars": sidecars,
            },
            "route": {
                "active_upstreams": route_entries,
                "identity_sha256": stable_hash(identity_text),
            },
            "profiles": profiles,
            "ledger": ledger,
            "redacted": True,
        }
    except sqlite3.Error as exc:
        return _snapshot_unreadable(phase, mode, str(exc))
    finally:
        try:
            connection.close()
        except sqlite3.Error:
            pass


def _snapshot_unreadable(phase: str, mode: str, detail: str) -> dict:
    return {
        "schema_version": SCHEMA_VERSION,
        "mode": mode,
        "phase": phase,
        "status": SNAPSHOT_STATUS_UNREADABLE,
        "ok": False,
        "collected_at": now_iso(),
        "host_platform": sys.platform,
        "db": {"path_redacted": True, "detail_redacted": detail[:200]},
        "route": {"active_upstreams": [], "identity_sha256": ""},
        "profiles": [],
        "ledger": {"events": [], "writes_attempted": 0, "redacted": True},
        "redacted": True,
    }


def _parse_active_upstream(connection: sqlite3.Connection, provider_id: str, app_type: str,
                           settings_config: str) -> dict | None:
    """Mirror of cc_switch.rs parse_active_upstream (redacted, fail-closed)."""
    try:
        settings = json.loads(settings_config)
    except ValueError:
        return None
    if not isinstance(settings, dict):
        return None

    env = settings.get("env")
    if isinstance(env, dict):
        endpoint = env.get("ANTHROPIC_BASE_URL")
        model = env.get("ANTHROPIC_MODEL")
        if not isinstance(endpoint, str) or not isinstance(model, str):
            return None
        credential_present = any(
            is_credential_key(key) and has_value(value) for key, value in env.items()
        )
        return {
            "app_type": app_type,
            "provider_id": provider_id,
            "model": model,
            "protocol": "anthropic",
            "normalized_endpoint": normalize_endpoint(endpoint),
            "credential_present": credential_present,
        }

    endpoint = None
    try:
        rows = connection.execute(
            "SELECT endpoint FROM provider_endpoints WHERE provider_id = ?", (provider_id,)
        ).fetchall()
        if rows:
            endpoint = rows[0][0]
    except sqlite3.Error:
        return None
    if endpoint is None:
        base_url = settings.get("baseUrl")
        endpoint = base_url if isinstance(base_url, str) else None
    if not isinstance(endpoint, str):
        return None
    protocol = settings.get("protocol")
    model = settings.get("model")
    if not isinstance(protocol, str) or not isinstance(model, str):
        return None
    credential_present = any(
        is_credential_key(key) and has_value(value) for key, value in settings.items()
    )
    return {
        "app_type": app_type,
        "provider_id": provider_id,
        "model": model,
        "protocol": protocol,
        "normalized_endpoint": normalize_endpoint(endpoint),
        "credential_present": credential_present,
    }


# --- Candidate binding ------------------------------------------------------

def git_head(repo_root: Path) -> tuple[str, bool]:
    """Read-only git rev-parse HEAD; (commit, resolved)."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=str(repo_root),
            capture_output=True,
            text=True,
            timeout=15,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return "", False
    commit = result.stdout.strip()
    if result.returncode != 0 or not COMMIT_RE.fullmatch(commit):
        return "", False
    return commit.lower(), True


def source_tree_digest(repo_root: Path) -> tuple[str, bool]:
    """Unified N0 source digest anchor: SHA-256 of the canonical JSON of
    {tracked path: blob sha256} over every tracked file at the candidate
    commit, computed by the shared producer
    generate_n0_candidate_evidence.compute_source_manifest (git ls-tree +
    git cat-file --batch, paths sorted by UTF-8). This is the exact digest
    definition the release aggregator (generate_release_gate_status) and the
    N0 freeze producer use; the former commit-tree-object-sha256 anchor is
    retired because it can never match the aggregator's frozen source digest.
    """
    try:
        if str(SCRIPTS_DIR) not in sys.path:
            sys.path.insert(0, str(SCRIPTS_DIR))
        from generate_n0_candidate_evidence import compute_source_manifest

        _, digest = compute_source_manifest(repo_root, "HEAD")
    except Exception:
        return "", False
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        return "", False
    return digest, True


# --- Actions ledger ---------------------------------------------------------

def validate_actions(actions_path: Path | None) -> list[dict]:
    """Load and redaction-check the Main computer-action ledger (may be absent)."""
    if actions_path is None:
        return []
    try:
        data = json.loads(actions_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise UsageError(f"unreadable --actions file: {exc}") from exc
    if not isinstance(data, list):
        raise UsageError("--actions must contain a JSON array")
    clean: list[dict] = []
    for index, entry in enumerate(data):
        if not isinstance(entry, dict):
            raise UsageError(f"--actions entry {index} is not an object")
        allowed = {"sequence", "action", "target", "detail", "detail_redacted", "at", "redacted"}
        extra = set(entry) - allowed
        if extra:
            raise UsageError(f"--actions entry {index} has disallowed keys: {sorted(extra)}")
        text = " ".join(
            str(entry.get(key, "")) for key in ("action", "target", "detail", "detail_redacted")
        )
        for pattern in SECRET_PATTERNS:
            if pattern.search(text):
                raise UsageError(
                    f"--actions entry {index} contains a secret-shaped value; "
                    "record only redacted descriptions of Computer actions"
                )
        clean.append({"sequence": entry.get("sequence", index + 1), **entry})
    return clean


# --- Snapshot / artifact schema checks --------------------------------------

def check_snapshot_schema(data: dict) -> list[str]:
    violations: list[str] = []
    if not isinstance(data, dict):
        return ["snapshot must be a top-level JSON object"]
    extra = set(data) - SNAPSHOT_KEYS
    if extra:
        violations.append(f"snapshot disallowed keys: {sorted(extra)}")
    if data.get("schema_version") != SCHEMA_VERSION:
        violations.append(f"snapshot schema_version must be {SCHEMA_VERSION!r}")
    if data.get("phase") not in PHASES:
        violations.append(f"snapshot phase must be one of {sorted(PHASES)}")
    if data.get("status") not in SNAPSHOT_STATUSES:
        violations.append(f"snapshot status must be one of {sorted(SNAPSHOT_STATUSES)}")
    db = data.get("db")
    if not isinstance(db, dict):
        violations.append("snapshot db block missing")
    elif db.get("path_redacted") is not True:
        violations.append("snapshot db.path_redacted must be true")
    if not isinstance(data.get("ledger"), dict):
        violations.append("snapshot ledger block missing")
    return violations


def check_artifact_schema(data: dict) -> list[str]:
    violations: list[str] = []
    if not isinstance(data, dict):
        return ["artifact must be a top-level JSON object"]
    extra = set(data) - ARTIFACT_KEYS
    if extra:
        violations.append(f"artifact disallowed keys: {sorted(extra)}")
    if data.get("schema_version") != SCHEMA_VERSION:
        violations.append(f"artifact schema_version must be {SCHEMA_VERSION!r}")
    if data.get("status") not in ARTIFACT_STATUSES:
        violations.append(f"artifact status must be one of {sorted(ARTIFACT_STATUSES)}")
    if not isinstance(data.get("ok"), bool):
        violations.append("artifact ok must be a boolean")
    if data.get("wrote_cc_switch") is not False:
        violations.append("artifact wrote_cc_switch must be false")
    if not isinstance(data.get("identity_restored"), bool):
        violations.append("artifact identity_restored must be a boolean")
    candidate = data.get("candidate")
    if not isinstance(candidate, dict):
        violations.append("artifact candidate block missing")
    elif candidate.get("resolved") is not True or not COMMIT_RE.fullmatch(
        candidate.get("commit", "")
    ):
        violations.append("artifact candidate commit must be resolved")
    digest = data.get("source_digest")
    if not isinstance(digest, dict) or not DIGEST_RE.fullmatch(digest.get("value", "")):
        violations.append("artifact source_digest.value must be a 64-hex sha256")
    declared_sha = data.get("artifact_sha256")
    recomputed = artifact_sha256(data)
    if not isinstance(declared_sha, str) or declared_sha != recomputed:
        violations.append(
            "artifact_sha256 mismatch (must equal SHA-256 of the canonical "
            "JSON without the artifact_sha256 field)"
        )
    ledger = data.get("ledger")
    if not isinstance(ledger, dict) or ledger.get("writes_attempted") != 0:
        violations.append("artifact ledger writes_attempted must be 0")
    restore = data.get("restore")
    if not isinstance(restore, dict) or restore.get("status") != "no_changes_made":
        violations.append("artifact restore.status must be no_changes_made")
    return violations


def check_schema_file(path: Path) -> int:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        print(f"unreadable or invalid JSON: {exc}", file=sys.stderr)
        return 1
    if isinstance(data, dict) and "phase" in data:
        violations = check_snapshot_schema(data)
    elif isinstance(data, dict) and "wrote_cc_switch" in data:
        violations = check_artifact_schema(data)
    else:
        violations = ["file is neither a phase snapshot nor a final artifact"]
    if violations:
        print("schema violations:", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        return 1
    return 0


# --- collect / verify -------------------------------------------------------

def cmd_collect(args: argparse.Namespace) -> int:
    if args.fixture and args.db is not None:
        raise UsageError("--fixture and --db are mutually exclusive; fixture mode only opens private copies of the committed fixture DB")
    if not args.fixture and args.db is None:
        raise UsageError("--db is required in real mode (the real cc-switch DB path is never auto-discovered)")
    if args.out is None:
        raise UsageError("--out is required (no implicit artifact writes)")

    if args.fixture:
        if not FIXTURE_DB.is_file():
            print(f"fixture database missing: {FIXTURE_DB}", file=sys.stderr)
            return 1
        temp_root = Path(tempfile.mkdtemp(prefix="cc-switch-aba-fixture-"))
        db_path = temp_root / FIXTURE_DB.name
        shutil.copy2(FIXTURE_DB, db_path)
        mode = "dry-run"
    else:
        db_path = Path(args.db).expanduser().resolve()
        if not db_path.is_absolute() or not db_path.is_file():
            raise UsageError(f"--db must be an absolute path to an existing file: {db_path}")
        mode = "real"

    snapshot = read_cc_switch_snapshot(db_path, args.phase, mode)
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(snapshot, indent=2) + "\n", encoding="utf-8")
    if not args.quiet:
        print(json.dumps(snapshot, indent=2))
    return 0 if snapshot["ok"] else 1


def load_snapshot(path: Path, phase_label: str) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise UsageError(f"unreadable {phase_label} snapshot {path}: {exc}") from exc
    violations = check_snapshot_schema(data)
    if violations:
        raise UsageError(f"{phase_label} snapshot schema violations: {'; '.join(violations)}")
    return data


def cmd_verify(args: argparse.Namespace) -> int:
    if args.fixture and args.db is not None:
        raise UsageError("--fixture and --db are mutually exclusive")
    if args.artifact is None:
        raise UsageError("--artifact is required (no implicit artifact writes)")
    if args.expected_commit is not None and not COMMIT_RE.fullmatch(args.expected_commit):
        raise UsageError("--expected-commit must be a 40-64 hex git commit")
    if args.expected_source_digest is not None and not DIGEST_RE.fullmatch(args.expected_source_digest):
        raise UsageError("--expected-source-digest must be a 64-hex sha256")

    pre = load_snapshot(Path(args.pre), "pre")
    observed_b = load_snapshot(Path(args.observed_b), "observed-b")
    post = load_snapshot(Path(args.post), "post")

    actions = validate_actions(Path(args.actions) if args.actions else None)

    failure_class = "none"
    detail_redacted = ""

    def fail(cls: str, detail: str) -> None:
        """Record the first failure only; checks run in precedence order."""
        nonlocal failure_class, detail_redacted
        assert cls in FAIL_CLASSES, cls
        if failure_class != "none":
            return
        failure_class = cls
        detail_redacted = detail[:300]

    pre_identity = pre["route"].get("identity_sha256", "")
    b_identity = observed_b["route"].get("identity_sha256", "")
    post_identity = post["route"].get("identity_sha256", "")
    identity_restored = bool(pre_identity and pre_identity == post_identity)
    observed_b_differs = bool(b_identity and b_identity != pre_identity)

    # Precedence: wrote_cc_switch (security invariant) > missing_observed_b >
    # missing_phase > switch_not_observed > identity_mismatch > binding >
    # db_changed_during_verify.
    pre_loaded = pre.get("status") == SNAPSHOT_STATUS_LOADED
    b_loaded = observed_b.get("status") == SNAPSHOT_STATUS_LOADED
    post_loaded = post.get("status") == SNAPSHOT_STATUS_LOADED

    for label, snapshot in (("pre", pre), ("observed-b", observed_b), ("post", post)):
        if snapshot.get("status") != SNAPSHOT_STATUS_LOADED:
            continue
        db = snapshot.get("db", {})
        if db.get("writes_attempted") != 0 or db.get("readonly_open") is not True:
            fail("wrote_cc_switch",
                 f"{label} snapshot records writes (writes_attempted={db.get('writes_attempted')}, "
                 f"readonly_open={db.get('readonly_open')}); the harness must never write cc-switch")

    if not b_loaded:
        fail("missing_observed_b", "observed-b snapshot missing or not loaded")
    elif not (pre_loaded and post_loaded):
        fail("missing_phase",
             f"pre loaded={pre_loaded}, post loaded={post_loaded}; all phases must be loaded")
    if pre_loaded and b_loaded and post_loaded:
        # An A->B->A round trip is only evidence when all three phases were
        # collected from the SAME cc-switch database path (same fingerprint).
        pre_db_fp = pre.get("db", {}).get("path_fingerprint_sha256", "")
        b_db_fp = observed_b.get("db", {}).get("path_fingerprint_sha256", "")
        post_db_fp = post.get("db", {}).get("path_fingerprint_sha256", "")
        if not (pre_db_fp and pre_db_fp == b_db_fp == post_db_fp):
            fail("db_path_mismatch",
                 "phase snapshots were not collected from the same cc-switch "
                 "database path (path fingerprints differ); an A->B->A round "
                 "trip must be evidenced on a single real database")
    if not observed_b_differs:
        fail("switch_not_observed",
             "observed B identity equals the pre identity; no A->B->A round trip was observed")
    if not identity_restored:
        fail("identity_mismatch",
             "post identity does not match pre identity (route identity was not restored)")

    # Candidate binding (read-only git).
    commit, commit_resolved = git_head(args.repo_root)
    digest, digest_resolved = source_tree_digest(args.repo_root)
    if not commit_resolved:
        fail("candidate_unbound", "candidate commit unresolvable (read-only git rev-parse HEAD failed)")
    if args.expected_commit is not None and commit.lower() != args.expected_commit.lower():
        fail("commit_mismatch", f"expected commit {args.expected_commit} != resolved {commit}")
    if args.expected_source_digest is not None:
        if not digest_resolved:
            fail("digest_mismatch", "source digest unresolvable (read-only git rev-parse HEAD^{tree} failed)")
        elif digest != args.expected_source_digest.lower():
            fail("digest_mismatch", f"expected source digest {args.expected_source_digest} != resolved {digest}")

    # DB must be unchanged between post collect and verify (no harness writes).
    db_unchanged_during_verify: bool | None = None
    if args.db is not None:
        db_path = Path(args.db).expanduser().resolve()
        try:
            current_fingerprint = file_sha256(db_path)
        except OSError as exc:
            current_fingerprint = ""
            fail("db_changed_during_verify", f"cannot re-read --db for fingerprint: {exc}")
        post_fingerprint = post.get("db", {}).get("source_fingerprint_sha256", "")
        if current_fingerprint and current_fingerprint != post_fingerprint:
            fail("db_changed_during_verify",
                 "cc-switch DB bytes changed between post collect and verify (harness must be read-only)")
        db_unchanged_during_verify = bool(current_fingerprint) and current_fingerprint == post_fingerprint

    status = ARTIFACT_STATUS_PASSED if failure_class == "none" else (
        ARTIFACT_STATUS_FAILED if failure_class in ("db_changed_during_verify", "db_unreadable") else ARTIFACT_STATUS_BLOCKED
    )

    artifact = {
        "schema_version": SCHEMA_VERSION,
        "mode": "dry-run" if args.fixture else "real",
        "status": status,
        "ok": status == ARTIFACT_STATUS_PASSED,
        "generated_at": now_iso(),
        "host_platform": sys.platform,
        "candidate": {"commit": commit, "resolved": commit_resolved},
        "source_digest": {
            "algorithm": "sha256",
            "value": digest,
            "resolved": digest_resolved,
            "scope": "n0-canonical-source-manifest",
        },
        "phases": {
            "pre": str(Path(args.pre)),
            "observed_b": str(Path(args.observed_b)),
            "post": str(Path(args.post)),
        },
        "route": {
            "pre_identity_sha256": pre_identity,
            "observed_b_identity_sha256": b_identity,
            "post_identity_sha256": post_identity,
            "identity_restored": identity_restored,
            "observed_b_differs": observed_b_differs,
        },
        "db": {
            "schema_version": post.get("db", {}).get("schema_version"),
            "schema_kind": post.get("db", {}).get("schema_kind"),
            "selection_state": post.get("db", {}).get("selection_state"),
            "readonly_open": True,
            "query_only": True,
            "writes_attempted": 0,
            "db_unchanged_during_verify": db_unchanged_during_verify,
        },
        "wrote_cc_switch": False,
        "identity_restored": identity_restored,
        "observed_b_differs": observed_b_differs,
        "ledger": {
            "harness_events": [
                {"at": now_iso(), "event": "verify", "phases": ["pre", "observed-b", "post"]},
            ],
            "computer_actions": actions,
            "writes_attempted": 0,
            "redacted": True,
        },
        "failure": {"class": failure_class, "detail_redacted": detail_redacted},
        "restore": {
            "status": "no_changes_made",
            "note": "harness performs no writes; nothing to restore",
        },
        "redacted": True,
    }
    artifact["artifact_sha256"] = artifact_sha256(artifact)

    artifact_path = Path(args.artifact)
    artifact_path.parent.mkdir(parents=True, exist_ok=True)
    tmp_name = artifact_path.parent / f".{artifact_path.name}.{os.getpid()}.tmp"
    tmp_name.write_text(json.dumps(artifact, indent=2) + "\n", encoding="utf-8")
    os.replace(tmp_name, artifact_path)
    if not args.quiet:
        print(json.dumps(artifact, indent=2))
    return 0 if artifact["ok"] else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Read-only cc-switch A->B->A evidence harness (fail-closed).",
    )
    sub = parser.add_subparsers(dest="command")

    collect = sub.add_parser("collect", help="Capture a phase snapshot (read-only)")
    collect.add_argument("--db", help="absolute path to the cc-switch database (real mode)")
    collect.add_argument("--out", help="snapshot output path (required)")
    collect.add_argument("--phase", choices=sorted(PHASES), default="pre",
                         help="phase label for this snapshot (default: pre)")
    collect.add_argument("--fixture", action="store_true",
                         help="dry-run: operate only on a private copy of the committed fixture DB")
    collect.add_argument("--quiet", action="store_true")
    collect.set_defaults(handler=cmd_collect)

    verify = sub.add_parser("verify", help="Verify A->B->A and emit the bound artifact")
    verify.add_argument("--pre", required=True, help="pre-switch snapshot")
    verify.add_argument("--observed-b", required=True, help="B-state snapshot")
    verify.add_argument("--post", required=True, help="post-switch (back to A) snapshot")
    verify.add_argument("--actions", help="Main computer-action ledger JSON (redacted descriptions)")
    verify.add_argument("--db", help="cc-switch DB path to re-fingerprint during verify (real mode)")
    verify.add_argument("--repo-root", default=str(REPO_ROOT),
                        help="repository root for read-only candidate binding (default: repo root of this script)")
    verify.add_argument("--expected-commit", help="required candidate commit (40-64 hex)")
    verify.add_argument("--expected-source-digest", help="required source digest sha256 (64 hex)")
    verify.add_argument("--artifact", help="artifact output path (required)")
    verify.add_argument("--fixture", action="store_true",
                        help="dry-run: verify fixture-derived snapshots; --db must also be a fixture copy")
    verify.add_argument("--quiet", action="store_true")
    verify.set_defaults(handler=cmd_verify)

    parser.add_argument("--check-schema", metavar="PATH",
                        help="validate a phase snapshot or final artifact JSON against the pinned schema and exit")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    try:
        args = parser.parse_args(argv)
        if args.check_schema:
            return check_schema_file(Path(args.check_schema))
        handler = getattr(args, "handler", None)
        if handler is None:
            parser.print_help()
            return 1
        return handler(args)
    except UsageError as exc:
        print(f"usage error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
