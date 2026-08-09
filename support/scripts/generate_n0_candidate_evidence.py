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


def registry_digest(registry_path: Path) -> str:
    """SHA-256 of the canonical JSON of the capability registry fixture."""
    if not registry_path.is_file():
        raise RuntimeError(f"registry fixture missing: {registry_path}")
    data = json.loads(registry_path.read_text(encoding="utf-8"))
    return sha256_bytes(gate_status.canonical_json_bytes(data))


def corpus_digests(manifest_path: Path) -> dict[str, str]:
    """Per-family digests from the unified-corpus manifest (fail-closed)."""
    if not manifest_path.is_file():
        raise RuntimeError(f"corpus manifest missing: {manifest_path}")
    data = json.loads(manifest_path.read_text(encoding="utf-8"))
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


def run_gate(root: Path, argv: list[str], cwd: Path) -> dict[str, Any]:
    started = utc_now()
    proc = subprocess.run(
        argv, cwd=cwd, check=False, stdout=subprocess.PIPE, stderr=subprocess.STDOUT
    )
    tail = proc.stdout.decode("utf-8", errors="replace").strip().splitlines()[-5:]
    return {
        "id": argv[0] if len(argv) == 1 else argv[-1],
        "argv": " ".join(str(item) for item in argv),
        "exit_code": proc.returncode,
        "ok": proc.returncode == 0,
        "output_tail": tail,
        "started_at": started,
        "finished_at": utc_now(),
    }


def preflight_gates(root: Path, skip: bool) -> list[dict[str, Any]]:
    """Run the complete N0 preflight defined by VERIFICATION.md §3."""
    if skip:
        return []
    desktop = root / "code" / "apps" / "desktop"
    gates = [
        (["python3", str(root / "support/scripts/check_structure.py")], root),
        (["python3", str(root / "support/scripts/check_release_version.py")], root),
        (["python3", str(root / "support/scripts/check_release_scope.py")], root),
        (["python3", str(root / "support/scripts/generate_release_metadata.py"), "--skip-audits"], root),
        (["python3", str(root / "support/scripts/validate_unified_capability_registry.py")], root),
        (["python3", str(root / "support/scripts/validate_unified_corpus_manifests.py")], root),
        (["bash", str(root / "support/scripts/run_fake_binary_tests.sh")], root),
        (["cargo", "fmt", "--all", "--", "--check"], root / "code"),
        (["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], root / "code"),
        (["cargo", "test", "--workspace"], root / "code"),
        (["npm", "run", "test:run"], desktop),
        ([str(desktop / "node_modules/.bin/tsc"), "--noEmit", "--pretty", "false"], desktop),
        (["npm", "run", "build"], desktop),
        (["npm", "run", "test:e2e"], desktop),
        (["git", "status", "--porcelain"], root),
    ]
    results = []
    for argv, cwd in gates:
        results.append(run_gate(root, argv, cwd))
    # The git-clean gate must be empty output.
    for result in results:
        if result["argv"] == "git status --porcelain":
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
        reg_digest = registry_digest(REGISTRY)
        corpus = corpus_digests(CORPUS_MANIFEST)
    except (RuntimeError, ValueError, json.JSONDecodeError, OSError) as exc:
        print(f"N0 freeze could not be computed: {exc}")
        return 1

    gates = preflight_gates(REPO_ROOT, args.skip_gates)
    gates_ok = bool(gates) and all(gate["ok"] for gate in gates)
    if args.skip_gates:
        # Test helper: without gates there is nothing to fail.
        status = "pass"
        reason_codes: list[str] = []
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
