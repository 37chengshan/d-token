#!/usr/bin/env python3
"""Verify and stage the pinned headless codebase-memory release artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import tarfile
import tempfile
import time
import urllib.request
import zipfile
from pathlib import Path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def copy_member(archive: Path, member_name: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f"{destination.name}.tmp-{os.getpid()}")
    try:
        if zipfile.is_zipfile(archive):
            with zipfile.ZipFile(archive) as package, package.open(member_name) as source:
                with temporary.open("wb") as output:
                    shutil.copyfileobj(source, output)
        else:
            with tarfile.open(archive, "r:gz") as package:
                member = package.getmember(member_name)
                if not member.isfile():
                    raise ValueError(f"archive member is not a regular file: {member_name}")
                source = package.extractfile(member)
                if source is None:
                    raise ValueError(f"archive member cannot be read: {member_name}")
                with source, temporary.open("wb") as output:
                    shutil.copyfileobj(source, output)
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def download(url: str, destination: Path, expected_sha256: str | None = None) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f"{destination.name}.download-{os.getpid()}")
    curl = shutil.which("curl")
    if curl is not None:
        completed = subprocess.run(
            [
                curl,
                "--silent",
                "--show-error",
                "--fail",
                "--location",
                "--retry",
                "5",
                "--retry-all-errors",
                "--continue-at",
                "-",
                "--output",
                str(temporary),
                url,
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
            timeout=600,
        )
        curl_integrity_ok = completed.returncode == 0 and temporary.is_file()
        if curl_integrity_ok and expected_sha256 is not None:
            curl_integrity_ok = sha256(temporary) == expected_sha256
        if curl_integrity_ok:
            os.replace(temporary, destination)
            return
        temporary.unlink(missing_ok=True)

    last_error: Exception | None = None
    expected_length: int | None = None
    for attempt in range(1, 6):
        try:
            offset = temporary.stat().st_size if temporary.exists() else 0
            request = urllib.request.Request(url)
            if offset:
                request.add_header("Range", f"bytes={offset}-")
            with urllib.request.urlopen(request, timeout=120) as response:
                status = getattr(response, "status", 200)
                append = offset > 0 and status == 206
                if offset and not append:
                    offset = 0
                declared_length = response.headers.get("Content-Length")
                content_range = response.headers.get("Content-Range")
                if content_range and "/" in content_range:
                    total = content_range.rsplit("/", 1)[1]
                    if total.isdigit():
                        expected_length = int(total)
                elif declared_length is not None and not append:
                    expected_length = int(declared_length)
                mode = "ab" if append else "wb"
                with temporary.open(mode) as output:
                    shutil.copyfileobj(response, output)
                    output.flush()
                    os.fsync(output.fileno())
            actual_length = temporary.stat().st_size
            if expected_length is not None and actual_length != expected_length:
                raise ValueError(
                    f"download length mismatch: expected {expected_length}, got {actual_length}"
                )
            if expected_sha256 is not None:
                actual_sha256 = sha256(temporary)
                if actual_sha256 != expected_sha256:
                    raise ValueError(
                        f"download SHA-256 mismatch: expected {expected_sha256}, got {actual_sha256}"
                    )
            os.replace(temporary, destination)
            return
        except Exception as error:  # early network EOF may require explicit integrity checks
            last_error = error
            if expected_length is not None and temporary.exists():
                if temporary.stat().st_size > expected_length:
                    temporary.unlink(missing_ok=True)
            if attempt < 5:
                time.sleep(attempt)
    temporary.unlink(missing_ok=True)
    raise RuntimeError(f"failed to download verified artifact after 5 attempts: {last_error}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--artifact-dir", required=True, type=Path)
    parser.add_argument("--binary-output", required=True, type=Path)
    parser.add_argument("--license-output", required=True, type=Path)
    parser.add_argument("--notices-output", required=True, type=Path)
    parser.add_argument("--download", action="store_true")
    parser.add_argument("--require-attestation", action="store_true")
    args = parser.parse_args()

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    if manifest.get("schema_version") != 1 or manifest.get("upstream", {}).get("tag") != "v0.9.0":
        raise SystemExit("unsupported codebase-memory artifact manifest")
    forbidden = ("runtime_download", "upstream_installer", "upstream_ui", "auto_update")
    policy = manifest.get("runtime_policy", {})
    if any(policy.get(key) is not False for key in forbidden):
        raise SystemExit("codebase-memory runtime policy is not fail-closed")

    matches = [item for item in manifest.get("artifacts", []) if item.get("target") == args.target]
    if len(matches) != 1:
        raise SystemExit(f"no unique pinned codebase-memory artifact for {args.target}")
    spec = matches[0]
    archive = args.artifact_dir / spec["archive"]
    archive_hash = sha256(archive) if archive.is_file() else None
    if archive_hash != spec["archive_sha256"]:
        if not args.download:
            if archive_hash is None:
                raise SystemExit(f"pinned codebase-memory archive is missing: {archive}")
            raise SystemExit(
                f"codebase-memory archive SHA-256 mismatch for {archive.name}: "
                f"expected {spec['archive_sha256']}, got {archive_hash}"
            )
        repository = manifest["upstream"]["repository"].rstrip("/")
        url = f"{repository}/releases/download/{manifest['upstream']['tag']}/{spec['archive']}"
        try:
            download(url, archive, spec["archive_sha256"])
        except RuntimeError as error:
            raise SystemExit(str(error)) from error

    archive_hash = sha256(archive)
    if archive_hash != spec["archive_sha256"]:
        raise SystemExit(
            f"codebase-memory archive SHA-256 mismatch for {archive.name}: "
            f"expected {spec['archive_sha256']}, got {archive_hash}"
        )

    if args.require_attestation:
        bundle_name = spec.get("sigstore_bundle")
        if not bundle_name:
            raise SystemExit("pinned codebase-memory artifact has no attestation bundle")
        bundle = args.artifact_dir / bundle_name
        if not bundle.is_file():
            if not args.download:
                raise SystemExit(f"codebase-memory attestation bundle is missing: {bundle}")
            repository = manifest["upstream"]["repository"].rstrip("/")
            url = f"{repository}/releases/download/{manifest['upstream']['tag']}/{bundle_name}"
            try:
                download(url, bundle)
            except RuntimeError as error:
                raise SystemExit(str(error)) from error
        sigstore = manifest.get("sigstore", {})
        identity = sigstore.get("certificate_identity")
        issuer = sigstore.get("certificate_oidc_issuer")
        if (
            sigstore.get("bundle_format") != "cosign_message_signature_v0.3"
            or not isinstance(identity, str)
            or not identity
            or not isinstance(issuer, str)
            or not issuer
        ):
            raise SystemExit("pinned codebase-memory Sigstore identity policy is missing")
        try:
            verified = subprocess.run(
                [
                    "cosign",
                    "verify-blob",
                    "--bundle",
                    str(bundle),
                    "--certificate-identity",
                    identity,
                    "--certificate-oidc-issuer",
                    issuer,
                    str(archive),
                ],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
                timeout=120,
            )
        except (FileNotFoundError, subprocess.TimeoutExpired) as error:
            raise SystemExit(f"Sigstore artifact verification could not run: {error}") from error
        if verified.returncode != 0:
            raise SystemExit("Sigstore artifact verification failed")

    binary_member = "codebase-memory-mcp.exe" if args.target.endswith("windows-msvc") else "codebase-memory-mcp"
    with tempfile.TemporaryDirectory(prefix="dtoken-codebase-memory-") as temporary:
        extracted = Path(temporary) / binary_member
        copy_member(archive, binary_member, extracted)
        binary_hash = sha256(extracted)
        if binary_hash != spec["binary_sha256"]:
            raise SystemExit(
                f"codebase-memory binary SHA-256 mismatch for {args.target}: "
                f"expected {spec['binary_sha256']}, got {binary_hash}"
            )
        args.binary_output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(extracted, args.binary_output)
        if os.name != "nt":
            args.binary_output.chmod(args.binary_output.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    copy_member(archive, "LICENSE", args.license_output)
    copy_member(archive, "THIRD_PARTY_NOTICES.md", args.notices_output)
    print(json.dumps({
        "ok": True,
        "mode": "prepare-codebase-memory-artifact",
        "target": args.target,
        "upstream_version": manifest["upstream"]["tag"],
        "upstream_commit": manifest["upstream"]["commit"],
        "archive": str(archive),
        "archive_sha256": archive_hash,
        "binary": str(args.binary_output),
        "binary_sha256": binary_hash,
        "attestation_verified": args.require_attestation,
    }, separators=(",", ":")))


if __name__ == "__main__":
    main()
