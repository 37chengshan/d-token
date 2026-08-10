#!/usr/bin/env python3
"""N0 candidate freeze evidence producer (v0.0.1).

Generates, under ``support/artifacts/final-candidate/<C_final>/``:

  * ``source-manifest.json`` — tracked-file canonical manifest + ``source_digest``
  * ``N0-preflight.json``    — unified evidence envelope + version + real preflight gates
  * ``N0-provenance.json``   — unified evidence envelope + registry/corpus provenance

Envelope and digest rules follow docs/releases/v0.0.1/VERIFICATION.md §1 and
EVIDENCE.md §4 (fail-closed; nothing here trusts producer self-reports):

  * ``source_digest`` = SHA-256 of the canonical JSON of ``{path: sha256}`` over
    the tracked files at ``C_final`` (paths sorted by UTF-8; gitignored
    artifact / target / node_modules / build output never enters).
  * ``registry_digest`` = SHA-256 of the canonical JSON of
    ``support/fixtures/unified-capability-registry-v1.json``.
  * ``corpus_digests`` = per-family digests read from
    ``support/fixtures/unified-corpus/manifest.json`` (fixed families:
    benchmark/replay/fuzz/fault/quality/optical; missing families fail closed).
  * ``artifact_sha256`` = SHA-256 of the canonical JSON of the artifact WITHOUT
    the ``artifact_sha256`` field — the aggregator recomputes this exactly.

Digests are computed from the frozen commit object (``git ls-tree`` +
``git cat-file --batch``), never from the mutable working tree, so the
evidence stays bound to ``C_final`` even while the worktree is being edited.

The N0-preflight ``status`` is ``pass`` only when every preflight gate really
passed; otherwise it is ``failed`` with reason codes (the aggregator rejects
non-pass N0 evidence, which is the honest fail-closed outcome).

Run::

  python3 support/scripts/generate_n0_candidate_evidence.py \
      --candidate-commit <40-hex> \
      [--artifacts-root support/artifacts/final-candidate/<C_final>] \
      [--expected-version 0.0.1] [--producer-revision <rev>] \
      [--skip-gates] [--quiet]

Exit 0 on success (artifacts written), 1 when the freeze cannot be produced.
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import generate_release_gate_status as gate_status  # noqa: E402

REPO_ROOT = SCRIPTS_DIR.parents[1]
REGISTRY = REPO_ROOT / "support" / "fixtures" / "unified-capability-registry-v1.json"
CORPUS_MANIFEST = REPO_ROOT / "support" / "fixtures" / "unified-corpus" / "manifest.json"
FIXED_CORPUS_FAMILIES = ["benchmark", "replay", "fuzz", "fault", "quality", "optical"]

ENVELOPE_FIELDS = list(gate_status.ENVELOPE_FIELDS)
DEFAULT_PRODUCER_REVISION = "generate_n0_candidate_evidence.py@1"


def utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run_git(root: Path, *arguments: str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["git", *arguments],
        cwd=root,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def tracked_files_at(root: Path, commit: str) -> list[str]:
    """Tracked file paths at ``commit`` (gitignored/untracked never appear)."""
    proc = run_git(root, "ls-tree", "-r", "-z", "--name-only", commit)
    if proc.returncode != 0:
        raise RuntimeError(f"git ls-tree failed for {commit}: {proc.stderr.decode()!r}")
    return sorted(proc.stdout.decode("utf-8").split("\0"))


def blob_sha256_at(root: Path, commit: str, paths: list[str]) -> dict[str, str]:
    """SHA-256 of each tracked file's bytes at ``commit`` (deterministic)."""
    ls = run_git(root, "ls-tree", "-r", "-z", commit)
    if ls.returncode != 0:
        raise RuntimeError(f"git ls-tree failed for {commit}")
    entries: list[tuple[str, str]] = []
    for raw in ls.stdout.decode("utf-8").split("\0"):
        if not raw:
            continue
        # <mode> <type> <sha>\t<path>
        meta, _, path = raw.partition("\t")
        parts = meta.split(" ")
        if len(parts) == 3 and parts[1] == "blob":
            entries.append((path, parts[2]))
    if not entries:
        return {}
    # One --batch process for all blobs.
    payload = b"".join(f"{oid}\n".encode("ascii") for _, oid in entries)
    cat = subprocess.run(
        ["git", "cat-file", "--batch"],
        cwd=root,
        check=False,
        input=payload,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if cat.returncode != 0:
        raise RuntimeError(f"git cat-file --batch failed: {cat.stderr.decode()!r}")
    # Stream-parse: each object is "<oid> <type> <size>\n" followed by exactly
    # <size> content bytes and one trailing newline.  Content may itself
    # contain newlines, so line-splitting is never used.
    results: dict[str, str] = {}
    out = cat.stdout
    pos = 0
    for path, _oid in entries:
        nl = out.index(b"\n", pos)
        header = out[pos:nl]
        pos = nl + 1
        parts = header.split(b" ")
        if len(parts) != 3:
            raise RuntimeError(f"unexpected cat-file header for {path!r}: {header!r}")
        size = int(parts[2])
        blob = out[pos:pos + size]
        pos += size
        if pos < len(out) and out[pos:pos + 1] == b"\n":
            pos += 1
        results[path] = sha256_bytes(blob)
    return results


def compute_source_manifest(root: Path, commit: str) -> tuple[dict[str, str], str]:
    """Tracked-file canonical manifest + source_digest (VERIFICATION.md §1)."""
    paths = tracked_files_at(root, commit)
    hashes = blob_sha256_at(root, commit, paths)
    manifest = {path: hashes[path] for path in paths if path in hashes}
    canonical = gate_status.canonical_json_bytes(manifest)
    return manifest, sha256_bytes(canonical)


def json_blob_at(root: Path, commit: str, path: str) -> dict[str, Any]:
    proc = run_git(root, "show", f"{commit}:{path}")
    if proc.returncode != 0:
        raise RuntimeError(f"tracked JSON fixture missing at {commit}:{path}")
    data = json.loads(proc.stdout.decode("utf-8"))
    if not isinstance(data, dict):
        raise RuntimeError(f"tracked JSON fixture must be an object: {path}")
    return data


def registry_digest(root: Path, commit: str) -> str:
    """SHA-256 of the canonical registry JSON at the frozen commit."""
    data = json_blob_at(
        root, commit, "support/fixtures/unified-capability-registry-v1.json"
    )
    return sha256_bytes(gate_status.canonical_json_bytes(data))


def corpus_digests(root: Path, commit: str) -> dict[str, str]:
    """Per-family digests from the frozen commit corpus manifest."""
    data = json_blob_at(root, commit, "support/fixtures/unified-corpus/manifest.json")
    corpora = data.get("corpora")
    if not isinstance(corpora, dict):
        raise RuntimeError("corpus manifest has no 'corpora' object")
    digests: dict[str, str] = {}
    missing: list[str] = []
    for family in FIXED_CORPUS_FAMILIES:
        entry = corpora.get(family)
        if not isinstance(entry, dict) or not isinstance(entry.get("digest"), str):
            missing.append(family)
            continue
        digests[family] = entry["digest"]
    if missing:
        raise RuntimeError(
            f"corpus families missing from manifest (fail-closed): {missing}"
        )
    return digests


def run_gate(root: Path, spec: dict[str, Any]) -> dict[str, Any]:
    """Execute one gate spec and record the observed outcome (never a self-report)."""
    started = utc_now()
    gate_env = dict(os.environ)
    if spec.get("env"):
        gate_env.update(spec["env"])
    proc = subprocess.run(
        spec["argv"],
        cwd=spec.get("cwd", root),
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=gate_env,
    )
    tail = proc.stdout.decode("utf-8", errors="replace").strip().splitlines()[-5:]
    return {
        "id": spec["id"],
        "argv": " ".join(str(item) for item in spec["argv"]),
        "exit_code": proc.returncode,
        "ok": proc.returncode == 0,
        "output_tail": tail,
        "started_at": started,
        "finished_at": utc_now(),
    }


def preflight_gate_specs(root: Path, skip: bool) -> list[dict[str, Any]]:
    """Pure construction of the complete N0 preflight gate specs (VERIFICATION.md
    §3).  Never executes anything — callers (tests, orchestrators) inspect the
    specs; actual execution happens only in :func:`preflight_gates`."""
    if skip:
        return []
    import os

    desktop = root / "code" / "apps" / "desktop"
    browser = os.environ.get("E2E_CHROMIUM_EXECUTABLE_PATH")
    if not browser:
        system_chrome = Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
        playwright_chrome = next(
            Path.home().glob(
                "Library/Caches/ms-playwright/chromium-*/chrome-mac-*/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
            ),
            None,
        )
        candidate = system_chrome if system_chrome.is_file() else playwright_chrome
        browser = str(candidate) if candidate and candidate.is_file() else None
    e2e_env = dict(os.environ)
    if browser:
        e2e_env["E2E_CHROMIUM_EXECUTABLE_PATH"] = browser
    e2e_env["E2E_DISABLE_VIDEO"] = "1"
    return [
        {"id": "check_structure", "argv": ["python3", str(root / "support/scripts/check_structure.py")], "cwd": root},
        {"id": "check_release_version", "argv": ["python3", str(root / "support/scripts/check_release_version.py")], "cwd": root},
        {"id": "check_release_scope", "argv": ["python3", str(root / "support/scripts/check_release_scope.py")], "cwd": root},
        {"id": "generate_release_metadata", "argv": ["python3", str(root / "support/scripts/generate_release_metadata.py"), "--skip-audits"], "cwd": root},
        {"id": "validate_unified_capability_registry", "argv": ["python3", str(root / "support/scripts/validate_unified_capability_registry.py")], "cwd": root},
        {"id": "validate_unified_corpus_manifests", "argv": ["python3", str(root / "support/scripts/validate_unified_corpus_manifests.py")], "cwd": root},
        {"id": "run_fake_binary_tests", "argv": ["bash", str(root / "support/scripts/run_fake_binary_tests.sh")], "cwd": root},
        {"id": "cargo_fmt", "argv": ["cargo", "fmt", "--all", "--", "--check"], "cwd": root / "code"},
        {"id": "cargo_clippy", "argv": ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], "cwd": root / "code"},
        {"id": "cargo_clean_bridge", "argv": ["cargo", "clean", "-p", "dtoken-agent-bridge"], "cwd": root / "code"},
        {"id": "cargo_build_bridge", "argv": ["cargo", "build", "-p", "dtoken-agent-bridge", "--bin", "dtoken-agent-bridge"], "cwd": root / "code"},
        {"id": "cargo_test_workspace", "argv": ["cargo", "test", "--workspace"], "cwd": root / "code", "env": {"CARGO_INCREMENTAL": "0"}},
        {"id": "vitest", "argv": ["npm", "run", "test:run"], "cwd": desktop},
        {"id": "tsc", "argv": [str(desktop / "node_modules/.bin/tsc"), "--noEmit", "--pretty", "false"], "cwd": desktop},
        {"id": "npm_build", "argv": ["npm", "run", "build"], "cwd": desktop},
        {"id": "e2e", "argv": ["npm", "run", "test:e2e"], "cwd": desktop, "env": e2e_env},
        {"id": "git_clean", "argv": ["git", "status", "--porcelain"], "cwd": root},
    ]


def preflight_gates(root: Path, skip: bool) -> list[dict[str, Any]]:
    """Run the complete N0 preflight defined by VERIFICATION.md §3."""
    results = [run_gate(root, spec) for spec in preflight_gate_specs(root, skip)]
    # The git-clean gate must be empty output.
    for result in results:
        if result["id"] == "git_clean":
            result["ok"] = result["exit_code"] == 0 and not result["output_tail"]
            if not result["ok"]:
                result["output_tail"] = (result["output_tail"] or []) + [
                    "worktree is not clean (candidate freeze requires clean HEAD)"
                ]
    return results


def build_envelope(
    node_id: str,
    candidate_commit: str,
    source_digest: str,
    registry_digest_value: str,
    corpus: dict[str, str],
    *,
    version: str,
    producer_revision: str,
    started_at: str,
    finished_at: str,
    status: str,
    reason_codes: list[str],
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    import platform as _platform

    data: dict[str, Any] = {
        "schema_version": "1",
        "node_id": node_id,
        "status": status,
        "reason_codes": reason_codes,
        "candidate_commit": candidate_commit,
        "source_digest": source_digest,
        "dirty": False,
        "registry_digest": registry_digest_value,
        "corpus_digests": dict(sorted(corpus.items())),
        "started_at": started_at,
        "finished_at": finished_at,
        "platform": {
            "os": sys.platform,
            "arch": _platform.machine() or "unknown",
            "toolchain": f"python{sys.version_info.major}.{sys.version_info.minor}",
        },
        "producer_revision": producer_revision,
    }
    if node_id == "N0":
        data["version"] = version
    if extra:
        data.update(extra)
    data["artifact_sha256"] = gate_status.recompute_artifact_sha256(data)
    return data


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-commit", required=True,
                        help="Frozen candidate commit C_final (40 hex chars)")
    parser.add_argument("--artifacts-root", default=None,
                        help="Output dir (default support/artifacts/final-candidate/<C_final>)")
    parser.add_argument("--expected-version", default="0.0.1",
                        help="Release version carried on N0 artifacts (default 0.0.1)")
    parser.add_argument("--producer-revision", default=DEFAULT_PRODUCER_REVISION,
                        help="Producer revision recorded in every envelope")
    parser.add_argument("--skip-gates", action="store_true",
                        help="Skip running preflight gates (test helper only)")
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args(argv)

    import re as _re
    if not _re.fullmatch(r"[0-9a-f]{40}", args.candidate_commit):
        print(f"--candidate-commit must be 40 lowercase hex, got {args.candidate_commit!r}")
        return 1

    artifacts_root = Path(args.artifacts_root) if args.artifacts_root else (
        REPO_ROOT / "support" / "artifacts" / "final-candidate" / args.candidate_commit
    )
    started_at = utc_now()

    try:
        manifest, source_digest = compute_source_manifest(REPO_ROOT, args.candidate_commit)
        reg_digest = registry_digest(REPO_ROOT, args.candidate_commit)
        corpus = corpus_digests(REPO_ROOT, args.candidate_commit)
    except (RuntimeError, ValueError, json.JSONDecodeError, OSError) as exc:
        print(f"N0 freeze could not be computed: {exc}")
        return 1

    gates = preflight_gates(REPO_ROOT, args.skip_gates)
    gates_ok = bool(gates) and all(gate["ok"] for gate in gates)
    if args.skip_gates:
        status = "skipped"
        reason_codes = ["gates-skipped"]
    else:
        status = "pass" if gates_ok else "failed"
        reason_codes = (
            []
            if gates_ok
            else [f"gate-{gate['id']}-failed" for gate in gates if not gate["ok"]]
        )

    finished_at = utc_now()

    source_artifact = {
        "schema_version": "1",
        "kind": "source-manifest",
        "candidate_commit": args.candidate_commit,
        "source_digest": source_digest,
        "dirty": False,
        "file_count": len(manifest),
        "algorithm": (
            "sha256-of-canonical-json-of-{path:sha256}-over-tracked-files-at-commit-v1"
        ),
        "files": manifest,
    }
    source_artifact["artifact_sha256"] = gate_status.recompute_artifact_sha256(
        source_artifact
    )

    preflight = build_envelope(
        "N0",
        args.candidate_commit,
        source_digest,
        reg_digest,
        corpus,
        version=args.expected_version,
        producer_revision=args.producer_revision,
        started_at=started_at,
        finished_at=finished_at,
        status=status,
        reason_codes=reason_codes,
        extra={
            "digests": {
                "source_digest": source_digest,
                "registry_digest": reg_digest,
                "corpus_digests": corpus,
            },
            "gates": gates,
            "source_manifest_file": "source-manifest.json",
        },
    )

    provenance = build_envelope(
        "N0",
        args.candidate_commit,
        source_digest,
        reg_digest,
        corpus,
        version=args.expected_version,
        producer_revision=args.producer_revision,
        started_at=started_at,
        finished_at=finished_at,
        status=status,
        reason_codes=reason_codes,
        extra={
            "provenance": {
                "registry": {
                    "path": "support/fixtures/unified-capability-registry-v1.json",
                    "algorithm": "sha256-of-canonical-json-of-registry-v1",
                    "digest": reg_digest,
                },
                "corpora": {
                    "manifest": "support/fixtures/unified-corpus/manifest.json",
                    "families": FIXED_CORPUS_FAMILIES,
                    "digests": corpus,
                },
                "source": {
                    "algorithm": (
                        "sha256-of-canonical-json-of-{path:sha256}-over-"
                        "tracked-files-at-commit-v1"
                    ),
                    "file_count": len(manifest),
                },
            }
        },
    )

    artifacts_root.mkdir(parents=True, exist_ok=True)
    targets = {
        "source-manifest.json": source_artifact,
        "N0-preflight.json": preflight,
        "N0-provenance.json": provenance,
    }
    for name, data in targets.items():
        (artifacts_root / name).write_text(
            json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )

    if not args.quiet:
        print(
            f"N0 evidence written to {artifacts_root}\n"
            f"  candidate_commit={args.candidate_commit}\n"
            f"  source_digest={source_digest}\n"
            f"  registry_digest={reg_digest}\n"
            f"  corpus_digests={json.dumps(corpus)}\n"
            f"  status={status} reason_codes={reason_codes}"
        )
    return 0 if status == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
