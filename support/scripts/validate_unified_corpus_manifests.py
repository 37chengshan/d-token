#!/usr/bin/env python3
"""Contract validator for the unified corpus manifest (T-C0C / PR-C0C).

MUST (docs/releases/v0.0.1 QUALITY-3/4 / T-C0C):
  * the manifest carries schema/revision/digest/license/privacy and one corpus
    per fixed corpus family (benchmark/replay/fuzz/fault/quality/optical);
  * every file path resolves relative to the corpus fixture root;
  * every file sha256 matches the on-disk bytes;
  * every corpus digest is the SHA-256 of the canonical JSON (sorted keys, no
    extra whitespace) of its file entries — recomputed and compared exactly.

Run:  python3 support/scripts/validate_unified_corpus_manifests.py
Exit 0 on success, 1 with a report on any contract violation (fail-closed).
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
MANIFEST = REPO_ROOT / "support" / "fixtures" / "unified-corpus" / "manifest.json"
CORPUS_ROOT = MANIFEST.parent

EXPECTED_CORPORA = {"benchmark", "replay", "fuzz", "fault", "quality", "optical"}
REQUIRED_MANIFEST_KEYS = ["schema", "schema_version", "revision", "license", "privacy", "corpora"]
REQUIRED_CORPUS_KEYS = ["schema", "revision", "license", "privacy", "digest", "files"]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def corpus_digest(files: list[dict]) -> str:
    entries = [{"path": item["path"], "sha256": item["sha256"]} for item in files]
    entries.sort(key=lambda item: item["path"])
    canonical = json.dumps(
        {"files": entries}, sort_keys=True, separators=(",", ":")
    )
    return sha256_bytes(canonical.encode("utf-8"))


def validate(manifest_path: Path) -> list[str]:
    errors: list[str] = []
    if not manifest_path.exists():
        return [f"corpus manifest missing: {manifest_path}"]
    corpus_root = manifest_path.parent

    try:
        data = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return [f"corpus manifest is not valid JSON: {exc}"]

    for key in REQUIRED_MANIFEST_KEYS:
        if key not in data:
            errors.append(f"manifest missing required key '{key}'")

    corpora = data.get("corpora", {})
    for expected in sorted(EXPECTED_CORPORA):
        if expected not in corpora:
            errors.append(f"corpus family '{expected}' missing from manifest")

    for name, corpus in corpora.items():
        for key in REQUIRED_CORPUS_KEYS:
            if key not in corpus:
                errors.append(f"corpus '{name}': missing required key '{key}'")
        files = corpus.get("files", [])
        if not files:
            errors.append(f"corpus '{name}': files must be non-empty")
        for item in files:
            rel = item.get("path")
            if not rel:
                errors.append(f"corpus '{name}': file entry missing 'path'")
                continue
            path = (corpus_root / rel).resolve()
            if not path.is_file():
                errors.append(f"corpus '{name}': path does not resolve: {rel}")
                continue
            recorded = item.get("sha256")
            actual = sha256_bytes(path.read_bytes())
            if recorded != actual:
                errors.append(
                    f"corpus '{name}': digest mismatch for {rel}: "
                    f"recorded {recorded}, actual {actual}"
                )
        recorded_digest = corpus.get("digest")
        recomputed = corpus_digest(files)
        if recorded_digest != recomputed:
            errors.append(
                f"corpus '{name}': digest mismatch (recorded {recorded_digest}, "
                f"recomputed {recomputed})"
            )

    return errors


def main() -> int:
    manifest_path = Path(sys.argv[1]) if len(sys.argv) > 1 else MANIFEST
    errors = validate(manifest_path)
    if errors:
        print(f"corpus manifest contract FAILED ({len(errors)} violations):")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("corpus manifest contract OK: all paths resolve and digests match")
    return 0


if __name__ == "__main__":
    sys.exit(main())
