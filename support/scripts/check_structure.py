from __future__ import annotations

import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]

REQUIRED = [
    "README.md",
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
    "CHANGELOG.md",
    "SECURITY.md",
    "AGENTS.md",
    "CLAUDE.md",
    "docs/PRD.md",
    "docs/PLAN.md",
    "docs/index.md",
    "docs/DOCS_ORGANIZATION.md",
    "docs/TECH_STACK.md",
    "docs/ARCHITECTURE.md",
    "docs/SECURITY_BOUNDARIES.md",
    "docs/APP_FUNCTIONS_AND_PAGES.md",
    "docs/legal/THIRD_PARTY_NOTICES.md",
    "docs/legal/AIMAMI_APACHE_2_0_LICENSE.txt",
    "docs/product/IMPLEMENTATION_STATUS.md",
    "docs/research/FRONTEND_UI_PERFORMANCE_AUDIT_2026-07-15.md",
    "docs/research/BACKEND_PERFORMANCE_AUDIT_2026-07-15.md",
    "docs/research/PERFORMANCE_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-15.md",
    "docs/research/CODEBASE_MEMORY_MCP_INTEGRATION_RESEARCH_2026-07-16.md",
    "docs/archive/README.md",
    "docs/testing/TESTING_NO_TOUCH.md",
    "docs/testing/OPENCODE_LOCAL_TEST_PLAN.md",
    "docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md",
    "docs/optical/README.md",
    "docs/optical/implementation-status.md",
    "docs/optical/test-matrix.md",
    "docs/optical/release-gates.md",
    "docs/optical/review-round-5.md",
    "code/README.md",
    "code/Cargo.toml",
    "code/apps/desktop/README.md",
    "code/apps/desktop/package.json",
    "code/apps/desktop/src/app/lib/nav.ts",
    "code/apps/desktop/src-tauri/README.md",
    "code/apps/desktop/src-tauri/src/main.rs",
    "code/crates/dtoken-types/src/lib.rs",
    "code/crates/dtoken-config/src/lib.rs",
    "code/crates/dtoken-router/src/lib.rs",
    "code/crates/dtoken-compress/src/lib.rs",
    "code/crates/dtoken-process/src/lib.rs",
    "code/crates/dtoken-doctor/src/lib.rs",
    "code/crates/dtoken-event-ledger/src/lib.rs",
    "code/crates/dtoken-preprocessor/src/lib.rs",
    "code/crates/dtoken-headroom-adapter/src/lib.rs",
    "code/crates/dtoken-project-policy/src/lib.rs",
    "code/crates/dtoken-config-patcher/src/lib.rs",
    "code/crates/dtoken-core-api/src/lib.rs",
    "code/crates/dtoken-proxy/src/main.rs",
    "code/configs/config.example.json",
    "code/runtimes/headroom/README.md",
    "support/README.md",
    "support/git-operations.json",
    "support/pending-commits.json",
    "support/fixtures/README.md",
    "support/fake-bin/rtk",
    "support/fake-bin/headroom",
    "support/scripts/check_release_scope.py",
    "support/scripts/check_release_version.py",
    "support/scripts/generate_release_metadata.py",
    "support/scripts/run_macos_release_package_audit.sh",
    "support/scripts/run_fixture_tests.sh",
    "support/scripts/run_fixture_tests.ps1",
    "support/scripts/run_fake_binary_tests.sh",
    "support/scripts/run_windows_dacl_acl_tests.ps1",
    "support/scripts/run_nsis_upgrade_rollback_test.ps1",
    "support/scripts/run_wsl_rtk_integrity_test.ps1",
    "support/scripts/run_fan_in_artifact_contract.ps1",
    "support/scripts/generate_release_gate_status.py",
    "support/scripts/run_release_gate_contract_test.ps1",
    "support/tests/test_windows_dacl_acl_tests.py",
    "support/tests/test_nsis_upgrade_rollback.py",
    "support/tests/test_wsl_rtk_integrity.py",
    "support/tests/test_fan_in_artifact_contract.py",
    "support/tests/test_release_gate_status_aggregation.py",
]

FORBIDDEN = [
    "task_plan.md",
    "findings.md",
    "progress.md",
    "docs/prd/d-token-prd-v0.1.md",
    "docs/prd/d-token-prd-v0.2.md",
    "docs/prd/headroom-dashboard-full-checklist.md",
    "docs/prd/headroom-dashboard-migration-matrix.md",
    "docs/product/UI_CAPABILITY_MATRIX.md",
    "docs/product/RUNTIME_CAPABILITY_MATRIX.md",
    "docs/release/RELEASE_COMPLETION_PLAN.md",
    "docs/release/RELEASE_READINESS_REPORT.md",
    "docs/release/release-plan-2026-07",
    "docs/reviews/NO_TOUCH_TESTING_REVIEW_2026-07-01.md",
    "docs/reviews/PRD_MULTIDIMENSIONAL_GAP_REVIEW_2026-07-01.md",
    "docs/reviews/RESEARCH_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-01.md",
    "docs/notes/NOTES.md",
    "docs/notes/tech-notes.md",
    "docs/experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md",
    "docs/experiments/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md",
    "docs/optical/architecture.md",
    "docs/optical/review-round-1.md",
    "docs/optical/review-round-2.md",
    "docs/optical/review-round-3.md",
    "docs/optical/review-round-4.md",
    "support/automation-notes.md",
    "support/ci-notes.md",
    "support/ci-template-notes.md",
    "support/dev-notes.md",
    "support/dev/README.md",
    "support/done.md",
    "support/keep.md",
    "support/notes-index.md",
    "support/README_EXTRA.md",
    "support/repo-structure.md",
    "support/ROOT_LAYOUT.md",
    "support/root-cleanup-note.md",
    "support/status.md",
    "support/test-notes.md",
    "support/tests-notes.md",
    "support/PLACEHOLDER.md",
    "code/PLACEHOLDER.md",
    "apps/desktop/README.md",
    "crates/dtoken-proxy/README.md",
    "packages/sdk/README.md",
    "sidecars/cc-switch-bridge/README.md",
    "configs/providers.example.yaml",
    "scripts/check_structure.py",
]

ROOT_MARKDOWN_ALLOWED = {
    "README.md",
    "README-cn.md",
    "CHANGELOG.md",
    "SECURITY.md",
    "THIRD_PARTY_NOTICES.md",
    "AGENTS.md",
    "CLAUDE.md",
}
DOCS_ROOT_MARKDOWN_ALLOWED = {
    "APP_FUNCTIONS_AND_PAGES.md",
    "APP_PAGE_UX_SPEC.md",
    "ARCHITECTURE.md",
    "DOCS_ORGANIZATION.md",
    "PLAN.md",
    "PRD.md",
    "SECURITY_BOUNDARIES.md",
    "TECH_STACK.md",
    "agent-mcp-parallel-dev-prompts.md",
    "index.md",
    "agent-mcp-parallel-dev-prompts.md",
    "d-token_AI_product_target_spec_v2.md",
}

ACTIVE_MARKDOWN_ROOTS = [
    ROOT / "README.md",
    ROOT / "AGENTS.md",
    ROOT / "CLAUDE.md",
    ROOT / "docs",
    ROOT / "code",
    ROOT / "support" / "README.md",
]

LINK_PATTERN = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
MARKDOWN_SCAN_SKIPPED_DIRS = {
    ".git",
    "artifacts",
    "dist",
    "node_modules",
    "target",
}
MARKDOWN_SCAN_SKIPPED_PREFIXES = (
    "docs/archive/",
    "docs/history/",
    "code/apps/desktop/src-old/",
    "code/apps/desktop/src-tauri/binaries/codebase-memory/",
    "code/UI design for project/",
)


def markdown_scan_path_is_skipped(path: Path) -> bool:
    relative = path.relative_to(ROOT).as_posix().rstrip("/") + "/"
    return any(relative.startswith(prefix) for prefix in MARKDOWN_SCAN_SKIPPED_PREFIXES)


def active_markdown_files() -> list[Path]:
    files: list[Path] = []
    for entry in ACTIVE_MARKDOWN_ROOTS:
        if entry.is_file():
            files.append(entry)
            continue
        if not entry.exists():
            continue
        for current, directories, filenames in os.walk(entry, topdown=True):
            current_path = Path(current)
            if markdown_scan_path_is_skipped(current_path):
                directories.clear()
                continue
            directories[:] = [
                directory
                for directory in directories
                if directory not in MARKDOWN_SCAN_SKIPPED_DIRS
                and not markdown_scan_path_is_skipped(current_path / directory)
            ]
            files.extend(
                current_path / filename
                for filename in filenames
                if filename.endswith(".md")
            )
    return sorted(set(files))


def local_link_target(raw_target: str) -> str | None:
    target = raw_target.strip()
    if not target or target.startswith(("#", "http://", "https://", "mailto:")):
        return None
    if target.startswith("<") and target.endswith(">"):
        target = target[1:-1]
    target = target.split("#", 1)[0].strip()
    if not target:
        return None
    return target.replace("%20", " ")


def broken_markdown_links() -> list[str]:
    broken: list[str] = []
    for path in active_markdown_files():
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for raw_target in LINK_PATTERN.findall(text):
            target = local_link_target(raw_target)
            if target is None:
                continue
            resolved = (path.parent / target).resolve()
            try:
                resolved.relative_to(ROOT)
            except ValueError:
                broken.append(
                    f"{path.relative_to(ROOT).as_posix()} -> outside repository: {target}"
                )
                continue
            if not resolved.exists():
                broken.append(f"{path.relative_to(ROOT).as_posix()} -> {target}")
    return broken


missing = [path for path in REQUIRED if not (ROOT / path).exists()]
forbidden_present = [path for path in FORBIDDEN if (ROOT / path).exists()]

root_markdown = {path.name for path in ROOT.glob("*.md")}
unexpected_root_markdown = sorted(root_markdown - ROOT_MARKDOWN_ALLOWED)

docs_root_markdown = {path.name for path in (ROOT / "docs").glob("*.md")}
unexpected_docs_root_markdown = sorted(docs_root_markdown - DOCS_ROOT_MARKDOWN_ALLOWED)
missing_docs_root_markdown = sorted(DOCS_ROOT_MARKDOWN_ALLOWED - docs_root_markdown)

prd_sources = sorted(
    path.relative_to(ROOT).as_posix()
    for path in (ROOT / "docs").rglob("*.md")
    if path.name.casefold() == "prd.md" or path.name.casefold().startswith("d-token-prd-v")
)
unexpected_prd_sources = [path for path in prd_sources if path != "docs/PRD.md"]

broken_links = broken_markdown_links()

failures = any(
    [
        missing,
        forbidden_present,
        unexpected_root_markdown,
        unexpected_docs_root_markdown,
        missing_docs_root_markdown,
        unexpected_prd_sources,
        broken_links,
    ]
)

if failures:
    if missing:
        print("Missing required files:")
        for path in missing:
            print(f"  - {path}")
    if forbidden_present:
        print("Forbidden legacy/current-path files still present:")
        for path in forbidden_present:
            print(f"  - {path}")
    if unexpected_root_markdown:
        print("Unexpected Markdown files in repository root:")
        for path in unexpected_root_markdown:
            print(f"  - {path}")
    if unexpected_docs_root_markdown:
        print("Unexpected Markdown files in docs root:")
        for path in unexpected_docs_root_markdown:
            print(f"  - {path}")
    if missing_docs_root_markdown:
        print("Missing canonical docs root files:")
        for path in missing_docs_root_markdown:
            print(f"  - {path}")
    if unexpected_prd_sources:
        print("Unexpected parallel PRD files present:")
        for path in unexpected_prd_sources:
            print(f"  - {path}")
    if broken_links:
        print("Broken local Markdown links in active documentation:")
        for item in broken_links:
            print(f"  - {item}")
    raise SystemExit(1)

print(
    "Repository structure check passed: canonical roots, single PRD, archive boundaries, "
    "and active Markdown links are valid."
)
