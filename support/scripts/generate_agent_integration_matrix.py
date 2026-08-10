#!/usr/bin/env python3
"""Agent x integration-method matrix producer (generated, not duplicated).

Builds ``support/artifacts/agent-integration-matrix.json`` by *generating*
the matrix from the authoritative Rust registries instead of maintaining a
second catalog:

  * ``KNOWN_AGENT_ADAPTERS`` in
    ``code/crates/dtoken-config-patcher/src/agent.rs`` — the 23 Agent
    detection adapters (id / install_capability / session_capability);
  * ``ADAPTERS`` (known_agent_transport_adapters) in
    ``code/crates/dtoken-config-patcher/src/agent_transport.rs`` — the
    transport descriptors (can_configure / can_clear / integration_kind /
    advanced_required_reason / truthful_limitation);
  * ``agent_ids_for_cc_switch_app`` in
    ``code/apps/desktop/src-tauri/src/upstream_discovery.rs`` — the cc-switch
    app_type -> Agent mapping (claude/codex/gemini/opencode/openclaw).

The matrix is re-derived on every run; any drift between the registry and the
matrix (missing adapter, capability flip, new app_type without mapping) fails
closed instead of producing a stale artifact. The generated rows classify
each Agent exactly as the registries do: Route (verified write surface,
can_configure=can_clear=true), MCP/Hook (conditional write surface, requires
desktop-managed bridge identity verification), or detection-only (metadata,
never configured). OMP is a read-only observer: it consumes discovery and
route evidence and never writes Agent configs, so it appears in the matrix
with an explicit read-only contract anchored to the cc-switch app mapping
absence and the discovery source list.

Run::

  python3 support/scripts/generate_agent_integration_matrix.py [--out PATH]

Exit 0 when the matrix regenerates and satisfies every pinned invariant;
exit 1 otherwise (nothing is written on failure). No network, no config
writes, no cargo build: the registries are read as declarative source text.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
AGENT_RS = (
    REPO_ROOT / "code" / "crates" / "dtoken-config-patcher" / "src" / "agent.rs"
)
TRANSPORT_RS = (
    REPO_ROOT / "code" / "crates" / "dtoken-config-patcher" / "src" / "agent_transport.rs"
)
DISCOVERY_RS = (
    REPO_ROOT / "code" / "apps" / "desktop" / "src-tauri" / "src" / "upstream_discovery.rs"
)
DEFAULT_OUT = (
    REPO_ROOT / "support" / "artifacts" / "agent-integration-matrix.json"
)

# Pinned registry contract (REQUIREMENTS.md AGENT-4). Any drift fails closed.
EXPECTED_REGISTRY_COUNT = 23
EXPECTED_ROUTE_WRITE = 15
EXPECTED_CONDITIONAL_WRITE = 6  # MCP/Hook, desktop-managed bridge-verified
EXPECTED_DETECTION_ONLY = 2

# cc-switch app_type -> Agent mapping is read from the Rust match arms; the
# expected set below is the pinned contract the discovery SQL must cover.
EXPECTED_CC_SWITCH_APPS = {"claude", "codex", "gemini", "opencode", "openclaw"}

OMP_ROW = {
    "agent_id": "omp",
    "integration_method": "read-only",
    "method_kind": "ReadOnly",
    "can_configure": False,
    "can_clear": False,
    "provides_route_evidence": False,
    "modifies_model_provider": False,
    "cc_switch_app_type": None,
    "source": "omp-discovery-observer",
    "reason": (
        "OMP observes Agent/cc-switch discovery and route evidence and never "
        "writes Agent configuration files; it is generated into the matrix "
        "from the read-only consumer contract, not from a write registry."
    ),
}


class RegistryParseError(RuntimeError):
    pass


def parse_descriptor_blocks(text: str, struct_name: str) -> list[dict[str, Any]]:
    """Parse one declarative descriptor array literal from Rust source.

    Each descriptor is a struct literal with ``id: "..."`` plus scalar
    fields on their own lines (``field: value,``). Values are captured
    verbatim for bool/enum/string fields; nested braces are skipped by
    tracking brace depth from the descriptor's opening ``{``.
    """
    blocks: list[dict[str, Any]] = []
    pattern = re.compile(
        rf"{re.escape(struct_name)}\s*\{{(?P<body>.*?)\n\s*\}},",
        re.DOTALL,
    )
    for match in pattern.finditer(text):
        body = match.group("body")
        block: dict[str, Any] = {}
        for key in (
            "id", "can_configure", "can_clear", "integration_kind",
            "install_capability", "session_capability",
            "advanced_required_reason", "truthful_limitation",
        ):
            if key in ("can_configure", "can_clear"):
                found = re.search(rf"{key}\s*:\s*(true|false)", body)
                if found:
                    block[key] = found.group(1) == "true"
                continue
            if key in ("id", "install_capability", "session_capability",
                       "truthful_limitation"):
                found = re.search(
                    rf"{key}\s*:\s*(?:Some\()?\s*\"((?:[^\"\\\\]|\\\\.)*)\"",
                    body,
                )
                if found:
                    block[key] = found.group(1)
                continue
            if key == "integration_kind":
                found = re.search(
                    rf"{key}\s*:\s*[A-Za-z0-9_:]+::([A-Za-z0-9_]+)",
                    body,
                )
                if found:
                    block[key] = found.group(1)
                continue
                # Enum-valued fields: AgentIntegrationKind::Route,
                # AgentInstallCapability::OneClick, TruthfulLimitation::mcp().
                enum = re.search(
                    rf"{key}\s*:\s*(?:Some\()?\s*([A-Za-z_]+)::([A-Za-z_]+)",
                    body,
                )
                if enum:
                    block[key] = enum.group(2)
                continue
            if key == "advanced_required_reason":
                if re.search(rf"{key}\s*:\s*None", body):
                    block[key] = None
                    continue
                found = re.search(
                    rf"{key}\s*:\s*(?:Some\()?\s*\"((?:[^\"\\\\]|\\\\.)*)\"",
                    body,
                )
                if found:
                    block[key] = found.group(1)
                continue
        if "id" in block:
            blocks.append(block)
    return blocks


def load_registry(path: Path, struct_name: str) -> list[dict[str, Any]]:
    if not path.is_file():
        raise RegistryParseError(f"registry source missing: {path}")
    blocks = parse_descriptor_blocks(path.read_text(encoding="utf-8"), struct_name)
    ids = [block["id"] for block in blocks]
    if len(ids) != len(set(ids)):
        raise RegistryParseError(f"duplicate adapter ids in {path.name}")
    return blocks


def load_cc_switch_mapping() -> dict[str, list[str]]:
    text = DISCOVERY_RS.read_text(encoding="utf-8")
    match = re.search(
        r"fn agent_ids_for_cc_switch_app\(app: &str\) -> Vec<String> \{(?P<body>.*?)\n\}",
        text,
        re.DOTALL,
    )
    if not match:
        raise RegistryParseError("agent_ids_for_cc_switch_app not found")
    mapping: dict[str, list[str]] = {}
    for arm in re.finditer(r'(?m)^\s*"([a-z-]+)"\s*=>\s*vec!\[([^\]]*)\]', match.group("body")):
        app = arm.group(1)
        ids = re.findall(r'"([a-z-]+)"', arm.group(2))
        mapping[app] = ids
    if set(mapping) != EXPECTED_CC_SWITCH_APPS:
        raise RegistryParseError(
            f"cc-switch app mapping drift: {sorted(mapping)} != "
            f"{sorted(EXPECTED_CC_SWITCH_APPS)}"
        )
    return mapping


def build_matrix() -> dict[str, Any]:
    detection = load_registry(AGENT_RS, "AgentAdapterDescriptor")
    transport = load_registry(TRANSPORT_RS, "AgentTransportAdapterDescriptor")
    cc_switch = load_cc_switch_mapping()

    if len(detection) != EXPECTED_REGISTRY_COUNT:
        raise RegistryParseError(
            f"KNOWN_AGENT_ADAPTERS has {len(detection)} entries, expected "
            f"{EXPECTED_REGISTRY_COUNT}"
        )

    rows: list[dict[str, Any]] = []
    route_write = conditional_write = detection_only = 0
    for descriptor in transport:
        agent_id = descriptor["id"]
        can_configure = descriptor.get("can_configure") is True
        can_clear = descriptor.get("can_clear") is True
        integration_kind = str(descriptor.get("integration_kind", "unknown")).replace(
            "AgentIntegrationKind::", ""
        )
        advanced_required_reason = descriptor.get("advanced_required_reason")
        truthful = descriptor.get("truthful_limitation")
        # Route adapters carry truthful_limitation=None; route evidence is a
        # semantic property of the Route kind. MCP/Hook carry a limitation.
        provides_route_evidence = integration_kind == "Route"
        modifies_model_provider = integration_kind == "Route"
        if can_configure and can_clear and integration_kind == "Route":
            method = "route"
            method_kind = "Route"
            route_write += 1
        elif (
            isinstance(advanced_required_reason, str)
            and "detection remains metadata-only" in advanced_required_reason
        ):
            # The registry itself declares these adapters metadata-only: no
            # verified write surface exists, so they are never configured.
            method = "detection-only"
            method_kind = "DetectionOnly"
            detection_only += 1
        elif integration_kind in ("Mcp", "Hook"):
            method = integration_kind.lower()
            method_kind = "ConditionalManagedWrite"
            conditional_write += 1
        else:
            method = "detection-only"
            method_kind = "DetectionOnly"
            detection_only += 1
        cc_switch_apps = [
            app for app, ids in cc_switch.items() if agent_id in ids
        ]
        rows.append(
            {
                "agent_id": agent_id,
                "integration_method": method,
                "method_kind": method_kind,
                "can_configure": can_configure,
                "can_clear": can_clear,
                "integration_kind": integration_kind,
                "provides_route_evidence": provides_route_evidence,
                "modifies_model_provider": modifies_model_provider,
                "advanced_required_reason": (
                    advanced_required_reason if isinstance(advanced_required_reason, str) else None
                ),
                "cc_switch_app_type": cc_switch_apps,
            }
        )

    # OMP: read-only observer row (never a write registry member).
    rows.append(OMP_ROW)
    rows.sort(key=lambda row: row["agent_id"])

    if (route_write, conditional_write, detection_only) != (
        EXPECTED_ROUTE_WRITE,
        EXPECTED_CONDITIONAL_WRITE,
        EXPECTED_DETECTION_ONLY,
    ):
        raise RegistryParseError(
            f"matrix classification drift: route={route_write} conditional="
            f"{conditional_write} detection={detection_only}, expected "
            f"{EXPECTED_ROUTE_WRITE}/{EXPECTED_CONDITIONAL_WRITE}/"
            f"{EXPECTED_DETECTION_ONLY}"
        )

    return {
        "schema_version": 1,
        "generated": True,
        "source": "generated-from-rust-registries",
        "registry_counts": {
            "known_agent_adapters": len(detection),
            "transport_adapters": len(transport),
            "route_write": route_write,
            "conditional_write": conditional_write,
            "detection_only": detection_only,
            "omp_read_only": 1,
        },
        "cc_switch_app_types": sorted(cc_switch),
        "rows": rows,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default=str(DEFAULT_OUT))
    args = parser.parse_args(argv)
    try:
        matrix = build_matrix()
    except RegistryParseError as exc:
        print(f"matrix generation failed closed: {exc}", file=sys.stderr)
        return 1
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(matrix, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if not matrix["generated"]:
        print("matrix generation failed closed", file=sys.stderr)
        return 1
    print(f"matrix: {out} ({len(matrix['rows'])} rows)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
