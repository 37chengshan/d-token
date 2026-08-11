from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys
from typing import Any, Optional

ROOT = Path(__file__).resolve().parents[2]
DESKTOP = ROOT / "code" / "apps" / "desktop"
# Active release versions may be a plain SemVer release (e.g. 0.0.1) or a
# SemVer pre-release (e.g. 0.2.0-beta.1); both forms are accepted so the
# checker works for the v0.0.1 line as well as historical prerelease lines.
SEMVER_RELEASE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def toml_section(path: Path, name: str) -> str:
    lines = path.read_text(encoding="utf-8").splitlines()
    header = f"[{name}]"
    collecting = False
    collected: list[str] = []
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            if collecting:
                break
            collecting = stripped == header
            continue
        if collecting:
            collected.append(line)
    return "\n".join(collected)


def toml_string_value(path: Path, section: str, key: str) -> Optional[str]:
    body = toml_section(path, section)
    match = re.search(
        rf'^\s*{re.escape(key)}\s*=\s*"([^"]+)"\s*$',
        body,
        flags=re.MULTILINE,
    )
    return match.group(1) if match else None


def cargo_package_version(path: Path) -> Optional[str]:
    body = toml_section(path, "package")
    if not body:
        return None
    if re.search(r"^\s*version\.workspace\s*=\s*true\s*$", body, re.MULTILINE):
        return "workspace"
    match = re.search(r'^\s*version\s*=\s*"([^"]+)"\s*$', body, re.MULTILINE)
    return match.group(1) if match else None


def cargo_manifests() -> list[Path]:
    manifests: list[Path] = []
    code_root = ROOT / "code"
    for current, directories, filenames in os.walk(code_root, topdown=True):
        directories[:] = [
            directory
            for directory in directories
            if directory not in {".git", "dist", "node_modules", "target"}
        ]
        if "Cargo.toml" in filenames:
            manifests.append(Path(current) / "Cargo.toml")
    return sorted(manifests)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify that all d-token release version sources agree."
    )
    parser.add_argument(
        "--expected",
        help="Expected version. Defaults to code/apps/desktop/package.json.",
    )
    parser.add_argument(
        "--tag",
        help="Optional release tag to verify. Defaults to GITHUB_REF_NAME when set.",
    )
    args = parser.parse_args()

    package_json = load_json(DESKTOP / "package.json")
    expected = args.expected or package_json.get("version")
    failures: list[str] = []

    if not isinstance(expected, str) or not SEMVER_RELEASE.fullmatch(expected):
        failures.append(
            f"expected version must be a SemVer release or pre-release string, "
            f"got {expected!r}"
        )
        expected = str(expected)

    workspace_version = toml_string_value(
        ROOT / "code" / "Cargo.toml", "workspace.package", "version"
    )
    sources: list[tuple[str, object]] = [
        ("code/apps/desktop/package.json", package_json.get("version")),
        (
            "code/apps/desktop/src-tauri/tauri.conf.json",
            load_json(DESKTOP / "src-tauri" / "tauri.conf.json").get("version"),
        ),
        ("code/Cargo.toml [workspace.package]", workspace_version),
    ]

    package_lock = load_json(DESKTOP / "package-lock.json")
    sources.extend(
        [
            ("code/apps/desktop/package-lock.json", package_lock.get("version")),
            (
                "code/apps/desktop/package-lock.json packages['']",
                package_lock.get("packages", {}).get("", {}).get("version"),
            ),
        ]
    )

    for label, actual in sources:
        if actual != expected:
            failures.append(f"{label}: expected {expected!r}, got {actual!r}")

    workspace_manifest = (ROOT / "code" / "Cargo.toml").resolve()
    checked_packages = 0
    for manifest in cargo_manifests():
        if manifest.resolve() == workspace_manifest:
            continue
        version = cargo_package_version(manifest)
        if version is None:
            continue
        checked_packages += 1
        relative = manifest.relative_to(ROOT).as_posix()
        if version == "workspace":
            continue
        if version != expected:
            failures.append(
                f"{relative} [package.version]: expected workspace inheritance or "
                f"{expected!r}, got {version!r}"
            )

    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    if f"## [{expected}]" not in changelog:
        failures.append(f"CHANGELOG.md has no release heading for {expected}")

    tag = args.tag or os.environ.get("GITHUB_REF_NAME", "")
    if tag:
        expected_tag = f"v{expected}"
        if tag != expected_tag:
            failures.append(f"release tag: expected {expected_tag!r}, got {tag!r}")

    if failures:
        print("Release version check failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print(
        "Release version check passed: "
        f"{expected} is consistent across npm, Tauri, Cargo workspace, "
        f"{checked_packages} Rust packages, changelog"
        + (f", and tag {tag}." if tag else ".")
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
