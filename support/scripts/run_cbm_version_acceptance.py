#!/usr/bin/env python3
"""Codebase Memory per-version acceptance producer/executor (PR-CBM4).

Reads the v2 artifact manifest and produces the per-version acceptance plan
for the five official releases plus the approved prerelease pin. The
acceptance probes (indexing / graph query / call chain / dependency / impact)
run only post-N0, in a temporary project strictly outside the d-token
repository, with persistence disabled and an endpoint allowlist derived from
the manifest.

Modes:

- ``produce`` (default): no-touch planning receipt. Never downloads, never
  executes the upstream binary, never writes inside the repository (stdout or
  an explicit ``--output`` path outside the repo root). Emits a blocked N4
  envelope pre-N0 (no candidate freeze yet), so the receipt is consumable by
  the final verifier and fails closed.
- ``dry-run``: human-readable plan, no-touch.
- ``execute``: real post-N0 execution, gated. Requires the frozen N0 artifact
  (envelope-valid, matching candidate_commit/source_digest/registry_digest/
  corpus_digests), the manifest SHA-256, a caller-supplied verified binary
  (hash-checked against the manifest official binary_sha256, or a
  source-built binary explicitly labeled ``source-built, not official
  binary`` for versions without official assets), an optional Sigstore bundle
  (mandatory for official-binary provenance), and an output path (inside the
  repo only under ``support/artifacts/final-candidate/<candidate>/``).

Receipts are unified evidence envelopes (VERIFICATION.md §1):
schema_version, node_id=N4, status, reason_codes, candidate_commit,
source_digest, dirty=false, registry_digest, corpus_digests, started_at,
finished_at, platform, producer_revision, artifact_sha256 (recomputed as
SHA-256 of canonical JSON without the artifact_sha256 field).

Version identity comes only from tag/commit/hash; the upstream server.json
version field is never read.
"""

from __future__ import annotations

import sys

# No-touch producer: never leave bytecode caches in the repository.
sys.dont_write_bytecode = True

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

RECEIPT_SCHEMA_VERSION = "1"
NODE_ID = "N4"
PRODUCER_REVISION = "run_cbm_version_acceptance.py@1"

PROBES = (
    ("indexing", "index a temporary project with persistence disabled and repo write deny"),
    ("graph_query", "open the d-token-owned read-only graph projection and run graph queries"),
    ("call_chain", "trace call-chain queries against the indexed graph"),
    ("dependency", "run dependency queries against the indexed graph"),
    ("impact", "run impact/scope queries against the indexed graph"),
)

# Only GitHub is an allowed host; every concrete URL is derived from the
# manifest (repository + pinned tag + pinned asset names), never from input.
ALLOWED_HOSTS = frozenset({"github.com"})

HOST_TARGETS = {
    ("Darwin", "arm64"): "aarch64-apple-darwin",
    ("Darwin", "x86_64"): "x86_64-apple-darwin",
    ("Windows", "AMD64"): "x86_64-pc-windows-msvc",
    ("Windows", "ARM64"): "aarch64-pc-windows-msvc",
}

ENVELOPE_FIELDS = [
    "schema_version",
    "node_id",
    "status",
    "reason_codes",
    "candidate_commit",
    "source_digest",
    "dirty",
    "registry_digest",
    "corpus_digests",
    "started_at",
    "finished_at",
    "platform",
    "producer_revision",
    "artifact_sha256",
]

DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")
CANDIDATE_RE = re.compile(r"^[0-9a-f]{40}$")

EVIDENCE_ROOT_REL = Path("support") / "artifacts" / "final-candidate"


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def canonical_json_bytes(data: dict) -> bytes:
    """Canonical JSON: UTF-8, object keys sorted, no extra whitespace."""
    return json.dumps(data, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def recompute_artifact_sha256(data: dict) -> str:
    """artifact_sha256 rule: remove the field, canonical-JSON, SHA-256 hex."""
    body = {key: value for key, value in data.items() if key != "artifact_sha256"}
    return hashlib.sha256(canonical_json_bytes(body)).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def detect_target() -> str:
    system = platform.system()
    machine = platform.machine()
    key = (system, machine)
    if key not in HOST_TARGETS:
        raise SystemExit(f"unsupported host for codebase-memory acceptance: {system}/{machine}")
    return HOST_TARGETS[key]


def find_repo_root(start: Path) -> Path:
    probe = start.resolve()
    while True:
        if (probe / ".git").exists():
            return probe
        if probe.parent == probe:
            raise SystemExit(f"could not locate repository root above {start}")
        probe = probe.parent


def load_manifest(path: Path) -> Dict[str, Any]:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if manifest.get("schema_version") != 2:
        raise SystemExit("codebase-memory acceptance requires the v2 artifact manifest (schema_version=2)")
    identity = manifest.get("identity_policy", {})
    if identity.get("identity_sources") != ["tag", "commit", "sha256"]:
        raise SystemExit("codebase-memory v2 manifest identity must be tag/commit/sha256 only")
    if identity.get("server_json_version_used") is not False:
        raise SystemExit("codebase-memory v2 manifest must never read server.json version")
    policy = manifest.get("runtime_policy", {})
    for key in ("runtime_download", "upstream_installer", "upstream_ui", "auto_update", "auto_watch"):
        if policy.get(key) is not False:
            raise SystemExit(f"codebase-memory runtime policy is not fail-closed: {key}")
    versions = manifest.get("versions")
    if not isinstance(versions, list) or not versions:
        raise SystemExit("codebase-memory v2 manifest has no versions registry")
    for entry in versions:
        if not entry.get("tag") or not re.fullmatch(r"[0-9a-f]{40}", entry.get("commit", "")):
            raise SystemExit(f"codebase-memory v2 version identity incomplete: {entry.get('version')}")
        matrix = entry.get("platform_matrix")
        if not isinstance(matrix, dict) or not matrix:
            raise SystemExit(f"codebase-memory v2 version has no platform matrix: {entry.get('version')}")
    return manifest


def endpoint_allowlist(manifest: Dict[str, Any]) -> List[str]:
    repository = manifest["upstream"]["repository"].rstrip("/")
    tag = manifest["upstream"]["tag"]
    urls = [f"{repository}/releases/tag/{tag}"]
    for item in manifest.get("artifacts", []):
        urls.append(f"{repository}/releases/download/{tag}/{item['archive']}")
        bundle = item.get("sigstore_bundle")
        if bundle:
            urls.append(f"{repository}/releases/download/{tag}/{bundle}")
    for entry in manifest.get("versions", []):
        for item in entry.get("artifacts") or []:
            urls.append(f"{repository}/releases/download/{entry['tag']}/{item['archive']}")
            bundle = item.get("sigstore_bundle")
            if bundle:
                urls.append(f"{repository}/releases/download/{entry['tag']}/{bundle}")
    return sorted(set(urls))


def probe_command(probe_id: str, binary: Path, project: Path) -> List[str]:
    return [str(binary), "cli", probe_id, "--project", str(project)]


def version_acceptance(manifest: Dict[str, Any], target: str) -> List[Dict[str, Any]]:
    """Compute the per-version acceptance status for the current target.

    Blocked whenever the version has no official assets (source-tag-only
    registration), when the current target is incompatible, or when no unique
    pinned artifact exists for the target. Versions with official assets on a
    compatible platform get a concrete post-N0 probe plan (indexing/graph
    query/call chain/dependency/impact) and stay ``planned``/not executed
    until the post-N0 gate runs them; their manifest ``runtime_status``
    remains ``blocked`` until the real verification evidence binds.
    """
    entries: List[Dict[str, Any]] = []
    for entry in manifest["versions"]:
        version = entry["version"]
        matrix = entry.get("platform_matrix", {})
        compatibility = matrix.get(target)
        block_reasons: List[str] = []
        if entry.get("asset_status") == "source_tag_only":
            block_reasons.append("assets_unavailable")
            block_reasons.append("source_tag_only")
        if compatibility != "compatible":
            block_reasons.append("platform_incompatible")
        artifacts = entry.get("artifacts") or []
        target_artifacts = [item for item in artifacts if item.get("target") == target]
        if compatibility == "compatible" and len(target_artifacts) != 1:
            block_reasons.append("no_unique_artifact_for_target")

        plan: Optional[Dict[str, Any]] = None
        if not block_reasons:
            plan = {
                "probes": [
                    {
                        "id": probe_id,
                        "description": description,
                        "command": probe_command(probe_id, Path("codebase-memory-mcp"), Path("{project}")),
                        "env": {
                            "CBM_PERSISTENCE": "false",
                            "CBM_REPO_WRITE_DENY": "true",
                            "CBM_ENDPOINT_ALLOWLIST": "github.com",
                        },
                        "evidence": "per-probe receipt: command, exit code, output digest, zero repo writes",
                    }
                    for probe_id, description in PROBES
                ],
                "temp_project_policy": "temporary project created under the system temp directory, "
                "strictly outside the d-token repository, deleted after the receipt is written",
                "repo_zero_write": True,
                "persistence": False,
                "endpoint_allowlist": sorted(ALLOWED_HOSTS),
                "requires": ["post-n0-candidate-freeze", "C_final-source-digest-binding"],
                "manifest_runtime_status": entry.get("runtime_status"),
                "runtime_becomes_verified_after": "real probe evidence bound to C_final",
            }

        entries.append(
            {
                "version": version,
                "kind": entry.get("kind"),
                "tag": entry["tag"],
                "commit": entry["commit"],
                "asset_status": entry.get("asset_status"),
                "runtime_status": entry.get("runtime_status"),
                "target": target,
                "platform_compatibility": compatibility,
                "acceptance_status": "blocked" if block_reasons else "planned",
                "block_reasons": block_reasons,
                "plan": plan,
            }
        )
    return entries


def _gate_status_module():
    import importlib.util

    path = Path(__file__).resolve().parent / "generate_release_gate_status.py"
    spec = importlib.util.spec_from_file_location("generate_release_gate_status", path)
    module = importlib.util.module_from_spec(spec)
    assert spec is not None and spec.loader is not None
    spec.loader.exec_module(module)
    return module


def validate_n0_gate(n0_path: Path, expected: Dict[str, Any]) -> Tuple[bool, List[str]]:
    """Validate the frozen N0 artifact envelope against the expected candidate.

    The post-N0 gate is armed only when an envelope-valid N0 artifact for the
    exact candidate_commit/source_digest/registry_digest/corpus_digests exists.
    """
    if not n0_path.is_file():
        return False, ["N0 artifact file is missing"]
    try:
        data = json.loads(n0_path.read_text(encoding="utf-8-sig"))
        if not isinstance(data, dict):
            raise ValueError("artifact root is not an object")
    except (OSError, ValueError) as exc:
        return False, [f"N0 artifact is not readable JSON: {exc}"]
    gate_status = _gate_status_module()
    violations = gate_status.envelope_violations(data, expected)
    if data.get("node_id") != "N0":
        violations.append(f"node_id={data.get('node_id')!r} is not 'N0'")
    return (not violations), violations


def git_status_snapshot(repo_root: Path) -> str:
    import subprocess

    completed = subprocess.run(
        ["git", "-C", str(repo_root), "status", "--porcelain"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=60,
    )
    if completed.returncode != 0:
        raise SystemExit(f"git status failed in {repo_root}: {completed.stderr.decode(errors='replace')}")
    return completed.stdout.decode("utf-8", errors="replace")


def assert_output_outside_repo(output: Path, repo_root: Path, candidate: Optional[str]) -> None:
    try:
        output.resolve().relative_to(repo_root.resolve())
    except ValueError:
        return
    # Execute mode may write gitignored runtime evidence under the sanctioned
    # evidence root; produce/dry-run never write inside the repository.
    if candidate is not None and CANDIDATE_RE.fullmatch(candidate or ""):
        allowed = (repo_root / EVIDENCE_ROOT_REL / candidate).resolve()
        try:
            output.resolve().relative_to(allowed)
            return
        except ValueError:
            pass
    raise SystemExit(
        f"zero-write violation: --output {output} resolves inside the repository root {repo_root}"
    )


def build_receipt(
    manifest: Dict[str, Any],
    target: str,
    repo_root: Path,
    mode: str,
) -> Dict[str, Any]:
    versions = version_acceptance(manifest, target)
    counts = {
        "total": len(versions),
        "blocked": sum(1 for item in versions if item["acceptance_status"] == "blocked"),
        "planned": sum(1 for item in versions if item["acceptance_status"] == "planned"),
        "executed": 0,
    }
    started = utc_now()
    receipt: Dict[str, Any] = {
        "schema_version": RECEIPT_SCHEMA_VERSION,
        "node_id": NODE_ID,
        # Pre-N0 there is no frozen candidate: the planning receipt is
        # explicitly blocked so the final verifier fails it closed.
        "status": "blocked",
        "reason_codes": ["blocked_pre_n0", "no_candidate_freeze"],
        "candidate_commit": None,
        "source_digest": None,
        "dirty": False,
        "registry_digest": None,
        "corpus_digests": {},
        "started_at": started,
        "finished_at": started,
        "platform": {
            "os": sys.platform,
            "arch": platform.machine() or "unknown",
            "toolchain": f"python{sys.version_info.major}.{sys.version_info.minor}",
        },
        "producer_revision": PRODUCER_REVISION,
        "manifest_revision": manifest.get("manifest_revision"),
        "manifest_path": manifest.get("_path"),
        "mode": mode,
        "no_touch": True,
        "executed": False,
        "host": {"system": platform.system(), "machine": platform.machine(), "target": target},
        "zero_write": {
            "repo_root": str(repo_root),
            "working_dir_policy": "temporary project outside the repository only",
            "output_policy": "stdout or an explicit --output path outside the repository root",
            "writes_in_repo": 0,
        },
        "endpoint_allowlist": endpoint_allowlist(manifest),
        "versions": versions,
        "counts": counts,
    }
    receipt["artifact_sha256"] = recompute_artifact_sha256(receipt)
    return receipt


def render_dry_run(receipt: Dict[str, Any]) -> str:
    lines = [
        f"Codebase Memory version acceptance (no-touch dry run, {receipt['host']['target']})",
        f"manifest revision: {receipt['manifest_revision']}",
        "",
    ]
    for entry in receipt["versions"]:
        status = entry["acceptance_status"]
        if status == "blocked":
            lines.append(f"[blocked] {entry['version']}: {', '.join(entry['block_reasons'])}")
        else:
            lines.append(f"[planned] {entry['version']}: post-N0 probes")
            for probe in entry["plan"]["probes"]:
                lines.append(f"    - {probe['id']}: {' '.join(probe['command'])}")
    lines.append("")
    lines.append("no execution, no downloads, no repository writes (no_touch=true).")
    return "\n".join(lines)


def run_execute(args: argparse.Namespace, manifest: Dict[str, Any], repo_root: Path) -> int:
    import subprocess

    version = args.version
    entry = next((item for item in manifest["versions"] if item["version"] == version), None)
    if entry is None:
        raise SystemExit(f"unknown codebase-memory version: {version}")
    target = args.target or detect_target()
    matrix = entry.get("platform_matrix", {})
    compatibility = matrix.get(target)
    source_built = bool(args.source_built)
    reason_codes: List[str] = []
    started = utc_now()

    # Eligibility: assets/platform availability, fail-closed.
    if entry.get("asset_status") == "source_tag_only" and not source_built:
        return _write_blocked_execute(
            args, manifest, repo_root, entry, target, started,
            ["assets_unavailable", "source_tag_only"],
            "no official assets; source-built binary required for behavior evidence",
        )
    if compatibility != "compatible" and not source_built:
        return _write_blocked_execute(
            args, manifest, repo_root, entry, target, started,
            ["platform_incompatible"],
            f"platform matrix marks {target} incompatible for {version}",
        )

    # Caller-supplied verified binary.
    binary = Path(args.binary)
    if not binary.is_file():
        raise SystemExit(f"verified binary is missing: {binary}")
    manifest_artifacts = entry.get("artifacts") or []
    manifest_hash = next(
        (item.get("binary_sha256") for item in manifest_artifacts if item.get("target") == target),
        None,
    )
    declared = args.binary_sha256 or manifest_hash
    if not declared or not DIGEST_RE.fullmatch(declared):
        reason_codes.append("missing_declared_binary_sha256")
    actual_hash = sha256_file(binary)
    hash_ok = bool(declared) and actual_hash == declared
    if not hash_ok:
        # Stable reason for an exact-hash mismatch against the declared hash.
        reason_codes.append("binary_hash_mismatch")
    if source_built:
        provenance = "source-built, not official binary"
        if manifest_hash and declared == manifest_hash:
            reason_codes.append("source_built_binary_hash_collides_with_official")
    elif manifest_hash is not None and declared == manifest_hash:
        provenance = "official-binary"
    else:
        # Declared hash is not the manifest official hash: provenance
        # downgraded and additionally recorded (hash mismatch already covers
        # the failing case).
        provenance = "official-binary-unverified"
        if hash_ok:
            reason_codes.append("not_manifest_official_binary")

    # Source-built provenance: pinned source commit + build provenance are
    # required (a bare --source-built flag is never sufficient).
    build_provenance: Optional[str] = None
    build_provenance_sha256: Optional[str] = None
    if source_built:
        if not args.source_commit or not CANDIDATE_RE.fullmatch(args.source_commit):
            raise SystemExit("--source-commit (pinned source commit) is required with --source-built")
        if args.source_commit != entry["commit"]:
            raise SystemExit(
                f"refusing execute: --source-commit {args.source_commit} != pinned commit "
                f"{entry['commit']} for {version}"
            )
        if not args.build_provenance:
            raise SystemExit(
                "--build-provenance is required with --source-built "
                "(path to a build provenance file or inline JSON)"
            )
        provenance_path = Path(args.build_provenance)
        if provenance_path.is_file():
            build_provenance = provenance_path.read_text(encoding="utf-8", errors="replace")
        else:
            build_provenance = args.build_provenance
        if not build_provenance.strip():
            raise SystemExit("--build-provenance must be non-empty")
        build_provenance_sha256 = hashlib.sha256(build_provenance.encode("utf-8")).hexdigest()

    # Signature evidence: mandatory for official-binary provenance.
    sigstore_ok = True
    if not source_built:
        bundle = Path(args.sigstore_bundle) if args.sigstore_bundle else None
        if bundle is None or not bundle.is_file():
            reason_codes.append("sigstore_bundle_required")
            sigstore_ok = False
        else:
            sigstore = manifest.get("sigstore", {})
            identity = sigstore.get("certificate_identity")
            issuer = sigstore.get("certificate_oidc_issuer")
            if sigstore.get("bundle_format") != "cosign_message_signature_v0.3" or not identity or not issuer:
                reason_codes.append("sigstore_identity_policy_missing")
                sigstore_ok = False
            else:
                cosign = shutil.which("cosign")
                if cosign is None:
                    reason_codes.append("sigstore_verification_unavailable")
                    sigstore_ok = False
                else:
                    completed = subprocess.run(
                        [
                            cosign, "verify-blob",
                            "--bundle", str(bundle),
                            "--certificate-identity", identity,
                            "--certificate-oidc-issuer", issuer,
                            str(binary),
                        ],
                        stdin=subprocess.DEVNULL,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        check=False,
                        timeout=300,
                    )
                    if completed.returncode != 0:
                        reason_codes.append("sigstore_verification_failed")
                        sigstore_ok = False

    # Post-N0 gate: frozen N0 artifact must bind the exact candidate.
    for field, value, pattern in (
        ("candidate_commit", args.candidate_commit, CANDIDATE_RE),
        ("source_digest", args.source_digest, DIGEST_RE),
        ("registry_digest", args.registry_digest, DIGEST_RE),
    ):
        if not value or not pattern.fullmatch(value):
            raise SystemExit(f"--{field} must be a valid hex digest, got {value!r}")
    try:
        corpus_digests = json.loads(args.corpus_digests) if args.corpus_digests else {}
    except json.JSONDecodeError as exc:
        raise SystemExit(f"--corpus-digests must be JSON: {exc}") from exc
    if not isinstance(corpus_digests, dict):
        raise SystemExit("--corpus-digests must be a JSON object")
    if not args.product_version:
        raise SystemExit("--product-version is required for execute mode")
    expected = {
        "commit": args.candidate_commit,
        "source_digest": args.source_digest,
        "registry_digest": args.registry_digest,
        "corpus_digests": corpus_digests,
        "version": args.product_version,
        "producer_revision": args.producer_revision,
    }
    if not args.n0_artifact:
        raise SystemExit("refusing execute: --n0-artifact is required (post-N0 gate)")
    gate_ok, gate_violations = validate_n0_gate(Path(args.n0_artifact), expected)
    if not gate_ok:
        raise SystemExit(
            "refusing execute: post-N0 gate not armed "
            f"(N0 artifact {args.n0_artifact} invalid: {'; '.join(gate_violations)})"
        )

    # Manifest digest binding.
    if not args.manifest_sha256 or not DIGEST_RE.fullmatch(args.manifest_sha256):
        raise SystemExit("--manifest-sha256 is required for execute mode")
    if sha256_file(Path(args.manifest_path)) != args.manifest_sha256:
        raise SystemExit("refusing execute: manifest SHA-256 mismatch")

    # Output path: sanctioned evidence root only when inside the repo.
    output = Path(args.output)
    assert_output_outside_repo(output, repo_root, args.candidate_commit)

    # Zero-write sentinel + temp project outside the repository.
    before = git_status_snapshot(repo_root)
    project = Path(tempfile.mkdtemp(prefix="dtoken-cbm-acceptance-")) / "project"
    project.mkdir(parents=True, exist_ok=True)
    (project / "README.md").write_text(
        "# synthetic acceptance project (no real prompts/code)\n", encoding="utf-8"
    )
    env = dict(os.environ)
    env.update(
        {
            "CBM_PERSISTENCE": "false",
            "CBM_REPO_WRITE_DENY": "true",
            "CBM_ENDPOINT_ALLOWLIST": "github.com",
        }
    )
    probe_results: List[Dict[str, Any]] = []
    try:
        for probe_id, description in PROBES:
            command = probe_command(probe_id, binary, project)
            try:
                completed = subprocess.run(
                    command,
                    cwd=str(project),
                    env=env,
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                    timeout=args.probe_timeout,
                )
                timed_out = False
            except subprocess.TimeoutExpired:
                completed = None
                timed_out = True
            if completed is not None:
                stdout = completed.stdout
                stderr = completed.stderr
                exit_code = completed.returncode
                output_sha256 = hashlib.sha256(stdout).hexdigest()
                ok = exit_code == 0
            else:
                stdout = b""
                stderr = b""
                exit_code = None
                output_sha256 = None
                ok = False
            if not ok:
                reason_codes.append(f"probe_failed:{probe_id}")
            probe_results.append(
                {
                    "id": probe_id,
                    "description": description,
                    "command": command,
                    "exit_code": exit_code,
                    "timed_out": timed_out,
                    "ok": ok,
                    "output_sha256": output_sha256,
                    "stdout_preview": stdout[:4000].decode("utf-8", errors="replace"),
                    "stderr_preview": stderr[:2000].decode("utf-8", errors="replace"),
                }
            )
    finally:
        shutil.rmtree(project.parent, ignore_errors=True)
    after = git_status_snapshot(repo_root)
    repo_unchanged = before == after
    if not repo_unchanged:
        reason_codes.append("repo_modified")
    finished = utc_now()

    status = "pass"
    if reason_codes:
        status = "failed"
    receipt: Dict[str, Any] = {
        "schema_version": RECEIPT_SCHEMA_VERSION,
        "node_id": NODE_ID,
        "status": status,
        "reason_codes": sorted(set(reason_codes)),
        "candidate_commit": args.candidate_commit,
        "source_digest": args.source_digest,
        "dirty": False,
        "registry_digest": args.registry_digest,
        "corpus_digests": corpus_digests,
        "started_at": started,
        "finished_at": finished,
        "platform": {
            "os": sys.platform,
            "arch": platform.machine() or "unknown",
            "toolchain": f"python{sys.version_info.major}.{sys.version_info.minor}",
        },
        "producer_revision": args.producer_revision,
        "version": args.product_version,
        "cbm_version": version,
        "cbm_tag": entry["tag"],
        "cbm_commit": entry["commit"],
        "provenance": provenance,
        "source_build": (
            {
                "source_commit": args.source_commit,
                "build_provenance": build_provenance[:8000],
                "build_provenance_sha256": build_provenance_sha256,
            }
            if source_built
            else None
        ),
        "runtime_offering_blocked": entry.get("runtime_status") != "verified",
        "manifest_digest_verified": True,
        "n0_gate": {
            "ok": gate_ok,
            "path": str(Path(args.n0_artifact).resolve()),
            "violations": gate_violations,
        },
        "binary": {
            "path": str(binary.resolve()),
            "sha256": actual_hash,
            "declared_sha256": declared,
            "hash_ok": hash_ok,
            "sigstore_ok": sigstore_ok,
        },
        "zero_write": {
            "repo_root": str(repo_root),
            "git_status_unchanged": repo_unchanged,
            "temp_project": str(project),
            "temp_project_removed": not project.exists(),
            "persistence": False,
            "repo_write_deny": True,
            "endpoint_allowlist": sorted(ALLOWED_HOSTS),
        },
        "probes": probe_results,
        "endpoint_allowlist": endpoint_allowlist(manifest),
        "counts": {"probes": len(probe_results), "passed": sum(1 for p in probe_results if p["ok"])},
    }
    receipt["artifact_sha256"] = recompute_artifact_sha256(receipt)

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"receipt written to {output} (status={status}, reason_codes={receipt['reason_codes']})")
    return 0 if status == "pass" else 1


def _write_blocked_execute(
    args: argparse.Namespace,
    manifest: Dict[str, Any],
    repo_root: Path,
    entry: Dict[str, Any],
    target: str,
    started: str,
    reason_codes: List[str],
    note: str,
) -> int:
    output = Path(args.output)
    assert_output_outside_repo(output, repo_root, getattr(args, "candidate_commit", None))
    finished = utc_now()
    receipt: Dict[str, Any] = {
        "schema_version": RECEIPT_SCHEMA_VERSION,
        "node_id": NODE_ID,
        "status": "blocked",
        "reason_codes": reason_codes,
        "candidate_commit": getattr(args, "candidate_commit", None),
        "source_digest": getattr(args, "source_digest", None),
        "dirty": False,
        "registry_digest": getattr(args, "registry_digest", None),
        "corpus_digests": {},
        "started_at": started,
        "finished_at": finished,
        "platform": {
            "os": sys.platform,
            "arch": platform.machine() or "unknown",
            "toolchain": f"python{sys.version_info.major}.{sys.version_info.minor}",
        },
        "producer_revision": getattr(args, "producer_revision", PRODUCER_REVISION),
        "cbm_version": entry["version"],
        "cbm_tag": entry["tag"],
        "cbm_commit": entry["commit"],
        "target": target,
        "note": note,
        "endpoint_allowlist": endpoint_allowlist(manifest),
    }
    receipt["artifact_sha256"] = recompute_artifact_sha256(receipt)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"blocked receipt written to {output} (reason_codes={reason_codes})")
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, help="v2 artifact manifest (default: repository copy)")
    parser.add_argument("--repo-root", type=Path, help="d-token repository root (default: auto-detected)")
    parser.add_argument("--platform", dest="target", help="Rust target triple (default: host detection)")
    parser.add_argument("--mode", choices=("produce", "dry-run"), default="produce")
    parser.add_argument("--output", type=Path, help="receipt JSON path (outside the repository root)")
    parser.add_argument(
        "--execute",
        action="store_true",
        help="post-N0 execution mode (gated on the frozen N0 artifact)",
    )
    parser.add_argument("--version", help="codebase-memory version to execute (one of the manifest versions)")
    parser.add_argument("--binary", type=Path, help="caller-supplied verified binary path")
    parser.add_argument("--binary-sha256", help="declared binary SHA-256 (default: manifest official hash)")
    parser.add_argument("--source-built", action="store_true", help="binary was built from the pinned source tag; provenance='source-built, not official binary'")
    parser.add_argument("--source-commit", help="pinned source commit (40 hex) the source-built binary was built from; must equal the manifest commit")
    parser.add_argument("--build-provenance", help="build provenance: path to a provenance file or inline JSON (toolchain, build command, builder identity)")
    parser.add_argument("--sigstore-bundle", type=Path, help="Cosign bundle for official-binary provenance")
    parser.add_argument("--candidate-commit", help="C_final candidate commit (40 hex)")
    parser.add_argument("--source-digest", help="canonical N0 source digest (64 hex)")
    parser.add_argument("--registry-digest", help="canonical capability-registry digest (64 hex)")
    parser.add_argument("--corpus-digests", help='JSON object of corpus digests, e.g. \'{"benchmark": "<64hex>"}\'')
    parser.add_argument("--product-version", help="product version bound by N0 (e.g. v0.0.1)")
    parser.add_argument("--producer-revision", default=PRODUCER_REVISION, help="producer revision recorded in the envelope")
    parser.add_argument("--n0-artifact", type=Path, help="frozen N0 evidence artifact path (post-N0 gate)")
    parser.add_argument("--manifest-sha256", help="declared SHA-256 of the v2 manifest (execute mode)")
    parser.add_argument("--probe-timeout", type=int, default=300, help="per-probe timeout seconds")
    args = parser.parse_args(argv)

    repo_root = (args.repo_root or find_repo_root(Path(__file__).resolve().parent)).resolve()
    manifest_path = (
        args.manifest
        or repo_root / "code" / "runtimes" / "codebase-memory" / "artifact-manifest.json"
    ).resolve()
    manifest = load_manifest(manifest_path)
    manifest["_path"] = str(manifest_path)
    args.manifest_path = str(manifest_path)
    target = args.target or detect_target()

    if args.execute:
        return run_execute(args, manifest, repo_root)

    if args.output is not None:
        assert_output_outside_repo(args.output, repo_root, None)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(build_receipt(manifest, target, repo_root, args.mode), indent=2, ensure_ascii=False)
            + "\n",
            encoding="utf-8",
        )
        print(f"receipt written to {args.output} (no_touch=true, executed=false)")
    else:
        receipt = build_receipt(manifest, target, repo_root, args.mode)
        if args.mode == "dry-run":
            print(render_dry_run(receipt))
        else:
            print(json.dumps(receipt, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
