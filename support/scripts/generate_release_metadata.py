from __future__ import annotations

import argparse
import csv
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
from typing import Any
from urllib.parse import quote
import uuid

ROOT = Path(__file__).resolve().parents[2]
CODE = ROOT / "code"
DESKTOP = CODE / "apps" / "desktop"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_command(command: list[str], cwd: Path) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=180,
        )
    except FileNotFoundError:
        return {
            "status": "unavailable",
            "returncode": None,
            "stdout": "",
            "stderr": f"command not found: {command[0]}",
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "status": "timeout",
            "returncode": None,
            "stdout": exc.stdout or "",
            "stderr": exc.stderr or "",
        }
    return {
        "status": "completed",
        "returncode": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def parse_json_output(output: str) -> dict[str, Any] | None:
    text = output.strip()
    if not text:
        return None
    try:
        parsed = json.loads(text)
        return parsed if isinstance(parsed, dict) else None
    except json.JSONDecodeError:
        start = text.find("{")
        end = text.rfind("}")
        if start < 0 or end <= start:
            return None
        try:
            parsed = json.loads(text[start : end + 1])
            return parsed if isinstance(parsed, dict) else None
        except json.JSONDecodeError:
            return None


def command_version(command: list[str], cwd: Path) -> str:
    result = run_command(command, cwd)
    if result["status"] != "completed" or result["returncode"] != 0:
        return "unavailable"
    output = (result["stdout"] or result["stderr"]).strip().splitlines()
    return output[0] if output else "unknown"


def npm_package_name(lock_path: str, metadata: dict[str, Any]) -> str | None:
    declared = metadata.get("name")
    if isinstance(declared, str) and declared:
        return declared
    marker = "node_modules/"
    if marker not in lock_path:
        return None
    return lock_path.rsplit(marker, 1)[-1] or None


def normalize_license(value: Any) -> str:
    if isinstance(value, str) and value.strip():
        return value.strip()
    if isinstance(value, list):
        items = [str(item).strip() for item in value if str(item).strip()]
        return " OR ".join(items) if items else "UNKNOWN"
    return "UNKNOWN"


def collect_npm_components() -> list[dict[str, Any]]:
    package_json = json.loads((DESKTOP / "package.json").read_text(encoding="utf-8"))
    package_lock = json.loads(
        (DESKTOP / "package-lock.json").read_text(encoding="utf-8")
    )
    runtime_direct = set(package_json.get("dependencies", {}))
    dev_direct = set(package_json.get("devDependencies", {}))
    components: list[dict[str, Any]] = []

    for lock_path, metadata in package_lock.get("packages", {}).items():
        if not lock_path or not isinstance(metadata, dict):
            continue
        name = npm_package_name(lock_path, metadata)
        version = metadata.get("version")
        if not name or not isinstance(version, str) or not version:
            continue
        scope = "transitive"
        if name in runtime_direct:
            scope = "runtime-direct"
        elif name in dev_direct:
            scope = "dev-direct"
        components.append(
            {
                "ecosystem": "npm",
                "name": name,
                "version": version,
                "license": normalize_license(metadata.get("license")),
                "source": str(metadata.get("resolved", "package-lock.json")),
                "scope": scope,
                "purl": f"pkg:npm/{quote(name, safe='@/')}@{quote(version, safe='.-')}",
            }
        )
    return components


def collect_cargo_components() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    result = run_command(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--manifest-path",
            str(CODE / "Cargo.toml"),
        ],
        ROOT,
    )
    if result["status"] != "completed" or result["returncode"] != 0:
        raise RuntimeError(
            "cargo metadata failed: "
            + (result["stderr"] or result["stdout"] or result["status"])
        )
    metadata = parse_json_output(result["stdout"])
    if metadata is None:
        raise RuntimeError("cargo metadata returned invalid JSON")

    workspace_members = set(metadata.get("workspace_members", []))
    components: list[dict[str, Any]] = []
    for package in metadata.get("packages", []):
        if not isinstance(package, dict):
            continue
        name = package.get("name")
        version = package.get("version")
        package_id = package.get("id")
        if not isinstance(name, str) or not isinstance(version, str):
            continue
        source = package.get("source") or package.get("manifest_path") or "Cargo.lock"
        components.append(
            {
                "ecosystem": "cargo",
                "name": name,
                "version": version,
                "license": normalize_license(package.get("license")),
                "source": str(source),
                "scope": "workspace" if package_id in workspace_members else "transitive",
                "purl": f"pkg:cargo/{quote(name, safe='-_.')}@{quote(version, safe='.-+')}",
            }
        )
    return components, metadata


def deduplicate_components(components: list[dict[str, Any]]) -> list[dict[str, Any]]:
    merged: dict[tuple[str, str, str], dict[str, Any]] = {}
    for component in components:
        key = (
            component["ecosystem"],
            component["name"],
            component["version"],
        )
        current = merged.get(key)
        if current is None:
            merged[key] = component
            continue
        if current["license"] == "UNKNOWN" and component["license"] != "UNKNOWN":
            current["license"] = component["license"]
        if current["scope"] == "transitive" and component["scope"] != "transitive":
            current["scope"] = component["scope"]
    return sorted(
        merged.values(),
        key=lambda item: (item["ecosystem"], item["name"].casefold(), item["version"]),
    )


def cyclonedx_component(component: dict[str, Any]) -> dict[str, Any]:
    result: dict[str, Any] = {
        "type": "library",
        "bom-ref": component["purl"],
        "name": component["name"],
        "version": component["version"],
        "purl": component["purl"],
        "properties": [
            {"name": "d-token:ecosystem", "value": component["ecosystem"]},
            {"name": "d-token:scope", "value": component["scope"]},
        ],
    }
    if component["license"] != "UNKNOWN":
        result["licenses"] = [{"expression": component["license"]}]
    return result


def extract_changelog(version: str) -> str:
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    pattern = re.compile(
        rf"^## \[{re.escape(version)}\][^\n]*\n(?P<body>.*?)(?=^## \[|\Z)",
        flags=re.MULTILINE | re.DOTALL,
    )
    match = pattern.search(changelog)
    if not match:
        raise RuntimeError(f"CHANGELOG.md has no section for {version}")
    body = match.group("body").strip()
    return f"# d-token {version}\n\n{body}\n"


def npm_security_summary() -> tuple[dict[str, Any], dict[str, Any]]:
    result = run_command(
        [
            "npm",
            "audit",
            "--json",
            "--registry=https://registry.npmjs.org",
        ],
        DESKTOP,
    )
    parsed = parse_json_output(result["stdout"])
    counts = {name: 0 for name in ("info", "low", "moderate", "high", "critical", "total")}
    vulnerabilities = None
    if parsed:
        candidate = parsed.get("metadata", {}).get("vulnerabilities")
        if isinstance(candidate, dict):
            vulnerabilities = candidate
            for name in counts:
                value = vulnerabilities.get(name, 0)
                if isinstance(value, int):
                    counts[name] = value

    if vulnerabilities is not None:
        status = "completed"
    elif result["status"] == "completed":
        status = "error" if result["returncode"] else "invalid-output"
    else:
        status = result["status"]

    return counts, {
        "status": status,
        "returncode": result["returncode"],
        "stderr": (result["stderr"] or "").strip()[:2000],
    }


def cargo_security_summary() -> tuple[dict[str, int], dict[str, Any]]:
    result = run_command(["cargo", "audit", "--json"], CODE)
    parsed = parse_json_output(result["stdout"])
    vulnerabilities = 0
    warnings = 0
    if parsed:
        vulnerability_list = parsed.get("vulnerabilities", {}).get("list", [])
        if isinstance(vulnerability_list, list):
            vulnerabilities = len(vulnerability_list)
        warning_map = parsed.get("warnings", {})
        if isinstance(warning_map, dict):
            warnings = sum(
                len(value) if isinstance(value, list) else 1
                for value in warning_map.values()
            )
    status = "completed" if parsed is not None else result["status"]
    stderr = (result["stderr"] or "").strip()
    if result["status"] == "completed" and parsed is None:
        if "no such command" in stderr.casefold():
            status = "unavailable"
        else:
            status = "invalid-output"
    return {"vulnerabilities": vulnerabilities, "warnings": warnings}, {
        "status": status,
        "returncode": result["returncode"],
        "stderr": stderr[:2000],
    }


def resolve_source_revision(explicit: str | None) -> str:
    """Commit-bound source digest: explicit value, CI SHA, then checked-out HEAD."""
    if explicit:
        return explicit
    if os.environ.get("GITHUB_SHA"):
        return os.environ["GITHUB_SHA"]
    commit = run_command(["git", "rev-parse", "HEAD"], ROOT)
    revision = str(commit.get("stdout", "")).strip()
    if commit.get("status") == "completed" and len(revision) == 40:
        return revision
    return "local-uncommitted"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate d-token release SBOM, license, security, notes and build metadata."
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=CODE / "target" / "release" / "metadata",
    )
    parser.add_argument("--skip-audits", action="store_true")
    parser.add_argument("--require-cargo-audit", action="store_true")
    parser.add_argument("--fail-on-high", action="store_true")
    parser.add_argument(
        "--source-revision",
        default=None,
        help="Commit-bound source digest; defaults to GITHUB_SHA, then the checked-out HEAD commit.",
    )
    parser.add_argument("--tag", default=os.environ.get("GITHUB_REF_NAME", ""))
    args = parser.parse_args()
    source_revision = resolve_source_revision(args.source_revision)

    package_json = json.loads((DESKTOP / "package.json").read_text(encoding="utf-8"))
    version = package_json["version"]
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    npm_components = collect_npm_components()
    cargo_components, cargo_metadata = collect_cargo_components()
    components = deduplicate_components(npm_components + cargo_components)

    lock_hashes = {
        "code/Cargo.lock": sha256_file(CODE / "Cargo.lock"),
        "code/apps/desktop/package-lock.json": sha256_file(DESKTOP / "package-lock.json"),
    }
    serial_seed = f"d-token:{version}:{lock_hashes['code/Cargo.lock']}:{lock_hashes['code/apps/desktop/package-lock.json']}"
    generated_at = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")

    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, serial_seed)}",
        "version": 1,
        "metadata": {
            "timestamp": generated_at,
            "tools": {
                "components": [
                    {
                        "type": "application",
                        "name": "d-token release metadata generator",
                        "version": version,
                    }
                ]
            },
            "component": {
                "type": "application",
                "name": "d-token",
                "version": version,
                "licenses": [{"license": {"id": "Apache-2.0"}}],
            },
            "properties": [
                {"name": "d-token:source-revision", "value": source_revision},
                {"name": "d-token:tag", "value": args.tag or "unassigned"},
            ],
        },
        "components": [cyclonedx_component(component) for component in components],
    }
    bom_path = output_dir / f"d-token-{version}.cdx.json"
    bom_path.write_text(json.dumps(bom, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    license_path = output_dir / "dependency-licenses.csv"
    with license_path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=("ecosystem", "name", "version", "license", "scope", "source"),
        )
        writer.writeheader()
        for component in components:
            writer.writerow({key: component[key] for key in writer.fieldnames})

    if args.skip_audits:
        npm_counts = {name: 0 for name in ("info", "low", "moderate", "high", "critical", "total")}
        npm_status = {"status": "skipped", "returncode": None, "stderr": ""}
        cargo_counts = {"vulnerabilities": 0, "warnings": 0}
        cargo_status = {"status": "skipped", "returncode": None, "stderr": ""}
    else:
        npm_counts, npm_status = npm_security_summary()
        cargo_counts, cargo_status = cargo_security_summary()

    security_path = output_dir / "security-scan.md"
    security_path.write_text(
        "\n".join(
            [
                "# Basic Security Scan Report",
                "",
                f"- Candidate: `d-token {version}`",
                f"- Generated: `{generated_at}`",
                f"- Source revision: `{source_revision}`",
                f"- Tag: `{args.tag or 'unassigned'}`",
                "",
                "## npm audit",
                "",
                f"- Status: `{npm_status['status']}`",
                f"- Exit code: `{npm_status['returncode']}`",
                f"- Info: {npm_counts['info']}",
                f"- Low: {npm_counts['low']}",
                f"- Moderate: {npm_counts['moderate']}",
                f"- High: {npm_counts['high']}",
                f"- Critical: {npm_counts['critical']}",
                f"- Total: {npm_counts['total']}",
                "",
                "## cargo audit",
                "",
                f"- Status: `{cargo_status['status']}`",
                f"- Exit code: `{cargo_status['returncode']}`",
                f"- RustSec vulnerabilities: {cargo_counts['vulnerabilities']}",
                f"- RustSec warnings: {cargo_counts['warnings']}",
                "",
                "## Scope and limitations",
                "",
                "This report covers dependency advisories visible to npm audit and cargo-audit at generation time. It is not a source-code penetration test, malware scan, platform signing assessment, secret scan, or guarantee that no vulnerability exists. Release publication still requires the full test, signing, package-content and installation gates.",
                "",
            ]
        ),
        encoding="utf-8",
    )

    release_notes_path = output_dir / "release-notes.md"
    release_notes_path.write_text(extract_changelog(version), encoding="utf-8")

    build_info = {
        "product": "d-token",
        "version": version,
        "tag": args.tag or None,
        "source_revision": source_revision,
        "generated_at": generated_at,
        "platform": platform.platform(),
        "architecture": platform.machine(),
        "runner_os": os.environ.get("RUNNER_OS"),
        "github_run_id": os.environ.get("GITHUB_RUN_ID"),
        "github_run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "tools": {
            "python": platform.python_version(),
            "node": command_version(["node", "--version"], ROOT),
            "npm": command_version(["npm", "--version"], ROOT),
            "rustc": command_version(["rustc", "--version"], ROOT),
            "cargo": command_version(["cargo", "--version"], ROOT),
        },
        "lockfile_sha256": lock_hashes,
        "dependency_components": {
            "total": len(components),
            "npm": len([item for item in components if item["ecosystem"] == "npm"]),
            "cargo": len([item for item in components if item["ecosystem"] == "cargo"]),
            "cargo_workspace_members": len(cargo_metadata.get("workspace_members", [])),
        },
        "security": {
            "npm": {"counts": npm_counts, **npm_status},
            "cargo": {"counts": cargo_counts, **cargo_status},
        },
        "artifacts": {
            "sbom": bom_path.name,
            "licenses": license_path.name,
            "security_scan": security_path.name,
            "release_notes": release_notes_path.name,
        },
    }
    build_info_path = output_dir / "build-info.json"
    build_info_path.write_text(
        json.dumps(build_info, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    failures: list[str] = []
    if args.require_cargo_audit and cargo_status["status"] != "completed":
        failures.append(f"cargo audit status is {cargo_status['status']}")
    if args.fail_on_high:
        if npm_status["status"] != "completed":
            failures.append(f"npm audit status is {npm_status['status']}")
        if npm_counts["high"] or npm_counts["critical"]:
            failures.append(
                "npm audit reported "
                f"{npm_counts['high']} high and {npm_counts['critical']} critical vulnerabilities"
            )
        if cargo_counts["vulnerabilities"]:
            failures.append(
                f"cargo audit reported {cargo_counts['vulnerabilities']} vulnerabilities"
            )

    print(f"Generated release metadata in {output_dir}")
    print(f"  - {bom_path.name}: {len(components)} components")
    print(f"  - {license_path.name}")
    print(f"  - {security_path.name}")
    print(f"  - {release_notes_path.name}")
    print(f"  - {build_info_path.name}")

    if failures:
        print("Release metadata security gate failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
