#!/usr/bin/env python3
"""Canonical tracked-file source digest shared by release evidence producers.

The source digest is SHA-256 of canonical JSON for ``{path: blob_sha256}`` over
all files tracked at the selected commit. Paths are UTF-8 sorted. Mutable
worktree bytes, untracked files, gitignored artifacts, Git tree object ids, and
legacy diff hashes never enter this value.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

COMMIT_RE = re.compile(r"^[0-9a-fA-F]{40,64}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")


class SourceDigestError(RuntimeError):
    """The candidate commit or canonical tracked-file manifest is unresolved."""


def _git(root: Path, *arguments: str) -> subprocess.CompletedProcess[bytes]:
    try:
        return subprocess.run(
            ["git", *arguments],
            cwd=root,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise SourceDigestError(f"git {' '.join(arguments)} failed: {exc}") from exc


def canonical_json_bytes(data: object) -> bytes:
    return json.dumps(
        data, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def resolve_commit(root: Path, revision: str = "HEAD") -> str:
    result = _git(root, "rev-parse", revision)
    commit = result.stdout.decode("utf-8", errors="replace").strip()
    if result.returncode != 0 or not COMMIT_RE.fullmatch(commit):
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise SourceDigestError(
            f"cannot resolve candidate commit {revision!r}"
            + (f": {detail}" if detail else "")
        )
    return commit.lower()


def tracked_files_at(root: Path, commit: str) -> list[str]:
    result = _git(root, "ls-tree", "-r", "-z", "--name-only", commit)
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise SourceDigestError(
            f"cannot list tracked files at {commit}"
            + (f": {detail}" if detail else "")
        )
    try:
        paths = result.stdout.decode("utf-8").split("\0")
    except UnicodeDecodeError as exc:
        raise SourceDigestError("tracked path is not valid UTF-8") from exc
    return sorted(path for path in paths if path)


def _tracked_blob_ids(root: Path, commit: str) -> dict[str, str]:
    result = _git(root, "ls-tree", "-r", "-z", commit)
    if result.returncode != 0:
        raise SourceDigestError(f"cannot resolve tracked blobs at {commit}")
    entries: dict[str, str] = {}
    try:
        records = result.stdout.decode("utf-8").split("\0")
    except UnicodeDecodeError as exc:
        raise SourceDigestError("tracked tree entry is not valid UTF-8") from exc
    for record in records:
        if not record:
            continue
        metadata, separator, path = record.partition("\t")
        parts = metadata.split(" ")
        if separator and len(parts) == 3 and parts[1] == "blob":
            entries[path] = parts[2]
    if not entries:
        raise SourceDigestError(f"candidate {commit} has no tracked blobs")
    return entries


def _blob_sha256(root: Path, entries: list[tuple[str, str]]) -> dict[str, str]:
    payload = b"".join(f"{oid}\n".encode("ascii") for _, oid in entries)
    try:
        process = subprocess.Popen(
            ["git", "cat-file", "--batch"],
            cwd=root,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        stdout, stderr = process.communicate(payload, timeout=120)
    except (OSError, subprocess.TimeoutExpired) as exc:
        if "process" in locals():
            process.kill()
            process.communicate()
        raise SourceDigestError(f"cannot read tracked blobs: {exc}") from exc
    if process.returncode != 0:
        raise SourceDigestError(
            "cannot read tracked blobs: "
            + stderr.decode("utf-8", errors="replace").strip()
        )

    results: dict[str, str] = {}
    position = 0
    for path, _ in entries:
        newline = stdout.find(b"\n", position)
        if newline < 0:
            raise SourceDigestError(f"truncated git cat-file header for {path}")
        header = stdout[position:newline].decode("ascii", errors="strict")
        parts = header.split(" ")
        if len(parts) != 3 or parts[1] != "blob" or not parts[2].isdigit():
            raise SourceDigestError(f"invalid git cat-file header for {path}")
        size = int(parts[2])
        position = newline + 1
        end = position + size
        if end > len(stdout):
            raise SourceDigestError(f"truncated tracked blob for {path}")
        results[path] = hashlib.sha256(stdout[position:end]).hexdigest()
        position = end
        if position < len(stdout) and stdout[position:position + 1] == b"\n":
            position += 1
    return results


def _compute_manifest_for_commit(root: Path, commit: str) -> tuple[dict[str, str], str]:
    paths = tracked_files_at(root, commit)
    blob_ids = _tracked_blob_ids(root, commit)
    entries = [(path, blob_ids[path]) for path in paths if path in blob_ids]
    if len(entries) != len(paths):
        missing = sorted(set(paths) - set(blob_ids))
        raise SourceDigestError(f"tracked blob ids missing for paths: {missing[:5]}")
    hashes = _blob_sha256(root, entries)
    manifest = {path: hashes[path] for path in paths}
    digest = hashlib.sha256(canonical_json_bytes(manifest)).hexdigest()
    if not DIGEST_RE.fullmatch(digest):
        raise SourceDigestError("canonical source digest is invalid")
    return manifest, digest


def compute_source_manifest(root: Path, revision: str = "HEAD") -> tuple[dict[str, str], str]:
    """Return the N0 canonical tracked-file manifest and its SHA-256 digest."""
    commit = resolve_commit(root, revision)
    return _compute_manifest_for_commit(root, commit)


def resolve_candidate_binding(root: Path, revision: str = "HEAD") -> tuple[str, str]:
    """Resolve exact candidate commit and N0 canonical source digest."""
    commit = resolve_commit(root, revision)
    _, digest = _compute_manifest_for_commit(root, commit)
    return commit, digest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--revision", default="HEAD")
    parser.add_argument("--digest-only", action="store_true")
    args = parser.parse_args()
    try:
        commit, digest = resolve_candidate_binding(args.repo_root, args.revision)
    except SourceDigestError as exc:
        import sys

        print(f"canonical source digest unresolved: {exc}", file=sys.stderr)
        return 1
    if args.digest_only:
        print(digest)
    else:
        print(json.dumps({"candidate_commit": commit, "source_digest": digest}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
