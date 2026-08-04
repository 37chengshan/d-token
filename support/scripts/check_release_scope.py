from __future__ import annotations

import subprocess
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]

FORBIDDEN_PREFIXES = (
    ".agents/",
    ".ai-bridge/",
    ".codex/",
    ".commandcode/",
    ".planning/",
    ".pytest_cache/",
    "headroom-",
    "support/artifacts/",
    "test/artifacts/",
    "code/target/",
)
FORBIDDEN_BASENAMES = {".DS_Store", "Thumbs.db"}
FORBIDDEN_SECRET_SUFFIXES = {".key", ".pem", ".p12", ".pfx"}
LOCAL_PATHS_EXPECTED_IGNORED = (
    ".agents/",
    ".ai-bridge/",
    ".codex/",
    ".commandcode/",
    ".planning/",
    ".pytest_cache/",
    "headroom-0.31.0/",
    "support/artifacts/",
    "test/artifacts/",
)


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        check=check,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def main() -> int:
    failures: list[str] = []
    try:
        tracked_raw = git("ls-files", "-z").stdout
    except (FileNotFoundError, subprocess.CalledProcessError) as exc:
        print(f"Release scope check could not read tracked files: {exc}")
        return 1

    tracked = [
        entry.decode("utf-8", errors="strict")
        for entry in tracked_raw.split(b"\0")
        if entry
    ]

    for path in tracked:
        if path.startswith(FORBIDDEN_PREFIXES):
            failures.append(f"tracked local/reference path: {path}")
        if Path(path).name in FORBIDDEN_BASENAMES:
            failures.append(f"tracked OS metadata file: {path}")
        if Path(path).suffix.casefold() in FORBIDDEN_SECRET_SUFFIXES:
            failures.append(f"tracked signing/secret material: {path}")

    for path in LOCAL_PATHS_EXPECTED_IGNORED:
        candidate = ROOT / path
        if not candidate.exists():
            continue
        result = git("check-ignore", "-q", "--", path, check=False)
        if result.returncode != 0:
            failures.append(f"existing local path is not ignored: {path}")

    required_release_files = (
        "LICENSE",
        "THIRD_PARTY_NOTICES.md",
        "CHANGELOG.md",
        "SECURITY.md",
        "README.md",
    )
    for path in required_release_files:
        if not (ROOT / path).is_file():
            failures.append(f"missing release file: {path}")

    if failures:
        print("Release scope check failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print(
        "Release scope check passed: "
        f"{len(tracked)} tracked files contain no blocked local/reference, "
        "OS metadata, or signing-secret paths; existing local workspaces are ignored."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
