#!/usr/bin/env python3
"""Deterministic cc-switch fixture database generator.

Builds ``support/fixtures/cc-switch/cc-switch.db`` with a fixed schema-12
provider/proxy/profile shape that mirrors the read-only surface consumed by
``dtoken-config-patcher`` / Tauri upstream discovery. The fixture is used only
for candidate-level auto-follow and route-sync contract tests: it must never
contain real credentials, real endpoints, or real user paths, and the
generation must be byte-stable so the committed file can be hash-verified.

Usage:
    python3 support/scripts/generate_cc_switch_fixture_db.py [--check]

``--check`` pins the committed file SHA-256 and compares a canonical
schema/row digest with a freshly generated database. SQLite file page layouts
can differ between library versions even when schema and rows are identical,
so cross-platform verification must not mistake physical-layout drift for
fixture-contract drift.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import sqlite3
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
FIXTURE_DIR = ROOT / "support" / "fixtures" / "cc-switch"
FIXTURE_PATH = FIXTURE_DIR / "cc-switch.db"
EXPECTED_FIXTURE_SHA256 = "9df5d00a215b8a3e6e14a73574bf0320d6989fcb7534bf6769f20b0862b91b97"
# Schema-16 fixture mirrors a real 2026-08 cc-switch desktop database layout
# (providers with meta/provider_type columns, provider_endpoints with a `url`
# column) including a chatgpt-auth Codex provider whose settings_config has no
# baseUrl/model — the per-app fail-closed contract case.
FIXTURE_V16_PATH = FIXTURE_DIR / "cc-switch-v16.db"
EXPECTED_FIXTURE_V16_SHA256 = "pending-first-generation"

# A deliberate secret-shaped marker that any leak path must never emit.
SECRET_MARKER = "fixture-cc-switch-token-marker-8f31"
# Real-looking but clearly synthetic endpoints for the fixture.
CLAUDE_ENDPOINT = "https://fixture-claude.example.test/v1"
CODEX_ENDPOINT = "https://fixture-codex.example.test/v1"
CODEX_ALT_ENDPOINT = "https://fixture-alt.example.test/v1"

SCHEMA = """
PRAGMA user_version = 12;
CREATE TABLE providers (
  id TEXT PRIMARY KEY,
  app_type TEXT NOT NULL,
  name TEXT NOT NULL,
  settings_config TEXT NOT NULL,
  is_current INTEGER NOT NULL DEFAULT 0,
  sort_index INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE provider_endpoints (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL,
  endpoint TEXT NOT NULL
);
CREATE TABLE proxy_config (
  app_type TEXT PRIMARY KEY,
  listen_port INTEGER NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0,
  proxy_enabled INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL
);
"""

PROVIDERS = [
    (
        "fixture-claude-current",
        "claude",
        "Fixture Claude current",
        json.dumps(
            {
                "env": {
                    "ANTHROPIC_BASE_URL": CLAUDE_ENDPOINT,
                    "ANTHROPIC_MODEL": "claude-fixture-model",
                    "ANTHROPIC_AUTH_TOKEN": SECRET_MARKER,
                },
                "private_metadata": SECRET_MARKER,
            },
            separators=(",", ":"),
        ),
        True,
        1,
    ),
    (
        "fixture-codex-current",
        "codex",
        "Fixture Codex current",
        json.dumps(
            {
                "baseUrl": CODEX_ENDPOINT,
                "protocol": "open_ai",
                "model": "codex-fixture-model",
            },
            separators=(",", ":"),
        ),
        True,
        1,
    ),
    (
        "fixture-codex-alt",
        "codex",
        "Fixture Codex alternative",
        json.dumps(
            {
                "baseUrl": CODEX_ALT_ENDPOINT,
                "protocol": "open_ai",
                "model": "codex-alt-model",
            },
            separators=(",", ":"),
        ),
        False,
        2,
    ),
    (
        "fixture-gemini-current",
        "gemini",
        "Fixture Gemini current",
        json.dumps(
            {
                "baseUrl": "https://fixture-gemini.example.test/v1",
                "protocol": "open_ai",
                "model": "gemini-fixture-model",
            },
            separators=(",", ":"),
        ),
        True,
        1,
    ),
]


def build_connection(path: pathlib.Path) -> sqlite3.Connection:
    connection = sqlite3.connect(path)
    connection.executescript(SCHEMA)
    for row in PROVIDERS:
        connection.execute(
            "INSERT INTO providers"
            " (id, app_type, name, settings_config, is_current, sort_index)"
            " VALUES (?, ?, ?, ?, ?, ?)",
            row,
        )
    connection.execute(
        "INSERT INTO proxy_config (app_type, listen_port, enabled, proxy_enabled)"
        " VALUES ('codex', 15721, 1, 1)"
    )
    connection.executemany(
        "INSERT INTO profiles (id, name) VALUES (?, ?)",
        [("fixture-profile-a", "Fixture Profile A"), ("fixture-profile-b", "Fixture Profile B")],
    )
    connection.commit()
    return connection


def sha256_of(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def generate(path: pathlib.Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary_name = tempfile.mkstemp(prefix=path.name + ".", dir=str(path.parent))
    os.close(fd)
    temporary = pathlib.Path(temporary_name)
    try:
        connection = build_connection(temporary)
        connection.close()
        temporary.replace(path)
    finally:
        if temporary.exists():
            temporary.unlink()


def canonical_digest(path: pathlib.Path) -> str:
    """Content digest that never builds SQL from identifiers.

    Both sides of the ``--check`` comparison run on the same host library, so
    a sorted ``iterdump()`` projection is a stable semantic fingerprint of the
    schema + rows without any dynamic table/column interpolation.
    """
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        user_version = connection.execute("PRAGMA user_version").fetchone()[0]
        dump_lines = sorted(connection.iterdump())
        payload = json.dumps(
            {"user_version": user_version, "dump": dump_lines},
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")
        return hashlib.sha256(payload).hexdigest()
    finally:
        connection.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the committed fixture hash")
    args = parser.parse_args()

    if args.check:
        if not FIXTURE_PATH.is_file():
            print(f"fixture missing: {FIXTURE_PATH}", file=sys.stderr)
            return 1
        with tempfile.TemporaryDirectory(prefix="cc-switch-fixture-") as directory:
            candidate = pathlib.Path(directory) / "cc-switch.db"
            generate(candidate)
            committed_sha256 = sha256_of(FIXTURE_PATH)
            generated_sha256 = sha256_of(candidate)
            committed_semantic = canonical_digest(FIXTURE_PATH)
            generated_semantic = canonical_digest(candidate)
            print(f"committed sha256: {committed_sha256}")
            print(f"generated  sha256: {generated_sha256}")
            print(f"semantic   sha256: {committed_semantic}")
            return (
                0
                if committed_sha256 == EXPECTED_FIXTURE_SHA256
                and committed_semantic == generated_semantic
                else 1
            )

    generate(FIXTURE_PATH)
    print(f"generated {FIXTURE_PATH}")
    print(f"sha256: {sha256_of(FIXTURE_PATH)}")
    print(f"secret marker (must never leak): {SECRET_MARKER}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
