"""Contract tests for the read-only cc-switch A->B->A evidence harness.

The harness (support/scripts/run_cc_switch_aba_check.py) collects snapshot /
route-identity / ledger evidence around the Main agent's real cc-switch
profile switch and emits an artifact bound to the candidate commit and source
digest.  These tests pin the contract that matters for release honesty
(docs/releases/v0.0.1/REQUIREMENTS.md AGENT-5 / SECURITY-7 / EVID-1):

  * the harness NEVER writes the cc-switch database: every phase snapshot
    records readonly_open=true, query_only=true, writes_attempted=0, and the
    raw DB bytes are unchanged by a collect;
  * the final artifact reports passed only for a genuine A->B->A round trip:
    a B state differing from A was observed, the post identity equals the pre
    identity, and wrote_cc_switch=false;
  * every failure path (identity not restored, no-op switch, missing B,
    tampered ledger, DB changed during verify, commit/digest mismatch) fails
    closed with a pinned failure class;
  * route signatures mirror the d-token-config-patcher pinned formats
    (cc_switch.rs / cc_switch_fixture.rs).

All tests run against private copies of the committed fixture database
(schema 12) or private synthetic schema-16 databases under a unique temp dir;
no real cc-switch database, agent config, or credential store is ever touched.

Run:  python3 support/tests/test_cc_switch_aba_check.py
"""
from __future__ import annotations

import hashlib
import json
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
HARNESS = REPO_ROOT / "support" / "scripts" / "run_cc_switch_aba_check.py"
FIXTURE_DB = REPO_ROOT / "support" / "fixtures" / "cc-switch" / "cc-switch.db"


def run_harness(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(HARNESS), *args], capture_output=True, text=True
    )


def run_collect(db: Path, phase: str, out: Path) -> subprocess.CompletedProcess:
    return run_harness("collect", "--db", str(db), "--phase", phase, "--out", str(out), "--quiet")


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_fixture(tmp: Path, name: str) -> Path:
    target = tmp / name
    shutil.copy2(FIXTURE_DB, target)
    return target


def flip_to_b(db: Path) -> None:
    """Flip a private copy to the B state (active codex provider alternates).

    This is the only write in the test suite and it targets a private temp
    copy — it simulates what the Main agent's Computer switch does to the real
    database between harness observations.  The file stays at the SAME path,
    so all three A->B->A phases share one path fingerprint.
    """
    conn = sqlite3.connect(str(db))
    conn.execute("UPDATE providers SET is_current = 0 WHERE app_type = 'codex' AND is_current = 1")
    conn.execute("UPDATE providers SET is_current = 1 WHERE id = 'fixture-codex-alt'")
    conn.commit()
    conn.close()


def make_schema16_db(tmp: Path, name: str) -> Path:
    """Private cc-switch database with the real-world schema 16 shape
    (PRAGMA user_version = 16).

    Mirrors the core tables the harness reads from the real
    ~/.cc-switch/cc-switch.db: profiles (payload column), providers
    (PK (id, app_type), is_current, sort_index, settings_config) and
    provider_endpoints (url column).  Providers are seeded with env-style
    settings_config (ANTHROPIC_BASE_URL / ANTHROPIC_MODEL) exactly like the
    real active claude provider, so the harness parses them via the env path
    and never touches the endpoint column.
    """
    db = tmp / name
    conn = sqlite3.connect(str(db))
    conn.execute("PRAGMA user_version = 16")
    conn.executescript(
        """
        CREATE TABLE profiles (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, payload TEXT NOT NULL,
            sort_order INTEGER, created_at INTEGER, updated_at INTEGER
        );
        CREATE TABLE providers (
            id TEXT NOT NULL, app_type TEXT NOT NULL, name TEXT NOT NULL,
            settings_config TEXT NOT NULL, website_url TEXT, category TEXT,
            created_at INTEGER, sort_index INTEGER, notes TEXT, icon TEXT,
            icon_color TEXT, meta TEXT NOT NULL DEFAULT '{}',
            is_current BOOLEAN NOT NULL DEFAULT 0,
            in_failover_queue BOOLEAN NOT NULL DEFAULT 0,
            cost_multiplier TEXT NOT NULL DEFAULT '1.0',
            limit_daily_usd TEXT, limit_monthly_usd TEXT, provider_type TEXT,
            PRIMARY KEY (id, app_type)
        );
        CREATE TABLE provider_endpoints (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id TEXT NOT NULL, app_type TEXT NOT NULL, url TEXT NOT NULL,
            added_at INTEGER,
            FOREIGN KEY (provider_id, app_type)
                REFERENCES providers(id, app_type) ON DELETE CASCADE
        );
        """
    )

    def insert_provider(pid: str, app_type: str, name: str, sort_index: int,
                        is_current: int, endpoint: str, model: str) -> None:
        conn.execute(
            "INSERT INTO providers (id, app_type, name, settings_config, "
            "sort_index, is_current, meta) VALUES (?, ?, ?, ?, ?, ?, '{}')",
            (pid, app_type, name,
             json.dumps({"env": {"ANTHROPIC_BASE_URL": endpoint,
                                 "ANTHROPIC_MODEL": model}}),
             sort_index, is_current),
        )

    insert_provider("v16-claude-current", "claude", "claude-current", 0, 1,
                    "https://v16-claude.example.test", "claude-v16-model")
    insert_provider("v16-codex-current", "codex", "codex-current", 0, 1,
                    "https://v16-codex.example.test", "codex-v16-model")
    insert_provider("v16-gemini-current", "gemini", "gemini-current", 0, 1,
                    "https://v16-gemini.example.test", "gemini-v16-model")
    insert_provider("v16-codex-alt", "codex", "codex-alt", 1, 0,
                    "https://v16-codex-alt.example.test", "codex-v16-alt-model")
    for pid, name in (("v16-profile-a", "profile-a"), ("v16-profile-b", "profile-b")):
        conn.execute("INSERT INTO profiles (id, name, payload) VALUES (?, ?, '{}')",
                     (pid, name))
    conn.execute(
        "INSERT INTO provider_endpoints (provider_id, app_type, url) "
        "VALUES ('v16-codex-alt', 'codex', 'https://v16-codex-alt.example.test')"
    )
    conn.commit()
    conn.close()
    return db


def flip_to_b_schema16(db: Path) -> None:
    """Flip a private schema-16 copy to the B state (active codex provider
    alternates), mirroring the Main agent's Computer switch on the real DB."""
    conn = sqlite3.connect(str(db))
    conn.execute("UPDATE providers SET is_current = 0 WHERE app_type = 'codex' AND is_current = 1")
    conn.execute("UPDATE providers SET is_current = 1 WHERE id = 'v16-codex-alt'")
    conn.commit()
    conn.close()


def flip_to_a_schema16(db: Path) -> None:
    """Restore the A state on the SAME path (active codex provider returns)."""
    conn = sqlite3.connect(str(db))
    conn.execute("UPDATE providers SET is_current = 0 WHERE app_type = 'codex' AND is_current = 1")
    conn.execute("UPDATE providers SET is_current = 1 WHERE id = 'v16-codex-current'")
    conn.commit()
    conn.close()


def collect_aba_phases(tmp: Path, db_name: str = "live.db") -> tuple[Path, Path, Path, Path]:
    """Collect pre / observed-b / post from the SAME database path.

    The test flips the file between collections (A -> B -> A), exactly like
    the Main agent's Computer switch on the real database.  Returns
    (pre, observed_b, post, db) paths.
    """
    db = copy_fixture(tmp, db_name)
    pre = tmp / "pre.json"
    b = tmp / "b.json"
    post = tmp / "post.json"
    for flip, phase, out in (
        (None, "pre", pre),
        (flip_to_b, "observed-b", b),
        (None, "post", post),
    ):
        if flip is not None:
            flip(db)
        elif phase == "post":
            # Restore the A state at the same path before the post collect.
            shutil.copy2(FIXTURE_DB, db)
        proc = run_collect(db, phase, out)
        assert proc.returncode == 0, f"collect {phase} failed: {proc.stderr}"
    return pre, b, post, db


def make_actions(tmp: Path, entries: list[dict] | None = None) -> Path:
    path = tmp / "computer-actions.json"
    path.write_text(json.dumps(entries or [
        {"sequence": 1, "action": "switch_cc_switch_profile", "target": "profile-b",
         "detail_redacted": True, "at": "2026-08-08T00:00:00Z"},
        {"sequence": 2, "action": "switch_cc_switch_profile", "target": "profile-a",
         "detail_redacted": True, "at": "2026-08-08T00:00:01Z"},
    ]), encoding="utf-8")
    return path


def repo_commit() -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=str(REPO_ROOT),
        capture_output=True, text=True, check=True,
    ).stdout.strip().lower()


def repo_digest() -> str:
    """Unified N0 source digest: SHA-256 of the canonical JSON of
    {tracked path: blob sha256} over every tracked file at HEAD (shared
    producer generate_n0_candidate_evidence.compute_source_manifest).

    Must mirror the harness's source_tree_digest exactly so bound-mode tests
    reproduce the producer value (invariant to worktree dirtiness).
    """
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
    from generate_n0_candidate_evidence import compute_source_manifest

    _, digest = compute_source_manifest(REPO_ROOT, "HEAD")
    return digest


def recompute_artifact_sha256(data: dict) -> str:
    """artifact_sha256 rule: SHA-256 of the canonical JSON without the field."""
    body = {key: value for key, value in data.items() if key != "artifact_sha256"}
    canonical = json.dumps(
        body, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(canonical).hexdigest()


def stable_hash(content: str) -> str:
    return hashlib.sha256(content.encode("utf-8")).hexdigest()


class CcSwitchAbaHarnessContractTests(unittest.TestCase):

    def test_harness_script_exists_and_parses(self) -> None:
        self.assertTrue(HARNESS.is_file(), "cc-switch ABA harness is missing")
        compile(HARNESS.read_text(encoding="utf-8"), str(HARNESS), "exec")
        self.assertTrue(FIXTURE_DB.is_file(), "committed cc-switch fixture is missing")

    def test_collect_fixture_snapshot_is_loaded_and_readonly(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = copy_fixture(tmp, "a.db")
            before = sha256_file(db)
            out = tmp / "pre.json"
            proc = run_collect(db, "pre", out)
            self.assertEqual(proc.returncode, 0)
            after = sha256_file(db)
            self.assertEqual(before, after, "collect must not modify the database bytes")
            snap = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(snap["status"], "loaded")
            self.assertEqual(snap["mode"], "real")
            self.assertEqual(snap["phase"], "pre")
            self.assertTrue(snap["ok"])
            self.assertEqual(snap["db"]["schema_version"], 12)
            self.assertEqual(snap["db"]["schema_kind"], "loaded")
            self.assertIs(snap["db"]["readonly_open"], True)
            self.assertIs(snap["db"]["query_only"], True)
            self.assertEqual(snap["db"]["writes_attempted"], 0)
            self.assertIs(snap["db"]["path_redacted"], True)
            self.assertTrue(snap["db"]["source_fingerprint_sha256"])
            self.assertTrue(snap["db"]["path_fingerprint_sha256"])
            self.assertEqual(snap["ledger"]["writes_attempted"], 0)
            self.assertIs(snap["redacted"], True)
            app_types = {u["app_type"] for u in snap["route"]["active_upstreams"]}
            self.assertEqual(app_types, {"claude", "codex", "gemini"})
            self.assertTrue(snap["route"]["identity_sha256"])

    def test_collect_fixture_mode_uses_private_copy(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            out = tmp / "pre.json"
            proc = run_harness("collect", "--fixture", "--phase", "pre", "--out", str(out), "--quiet")
            self.assertEqual(proc.returncode, 0)
            snap = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(snap["mode"], "dry-run")
            self.assertEqual(snap["status"], "loaded")

    def test_collect_unknown_schema_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = tmp / "unknown.db"
            conn = sqlite3.connect(str(db))
            conn.execute("PRAGMA user_version = 99")
            conn.execute("CREATE TABLE providers (id TEXT PRIMARY KEY)")
            conn.commit()
            conn.close()
            out = tmp / "snap.json"
            proc = run_collect(db, "pre", out)
            self.assertEqual(proc.returncode, 1)
            snap = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(snap["status"], "blocked")
            self.assertEqual(snap["db"]["schema_kind"], "unknown")
            self.assertEqual(snap["db"]["source_state"], "blocked")
            self.assertEqual(snap["route"]["active_upstreams"], [])
            self.assertEqual(snap["db"]["writes_attempted"], 0)

    def test_collect_schema16_snapshot_is_loaded_and_readonly(self) -> None:
        """The real cc-switch DB is user_version=16; collect must load it and
        record the real user_version, never a blocked snapshot."""
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = make_schema16_db(tmp, "v16.db")
            before = sha256_file(db)
            out = tmp / "pre.json"
            proc = run_collect(db, "pre", out)
            self.assertEqual(proc.returncode, 0)
            self.assertEqual(sha256_file(db), before,
                             "collect must not modify the database bytes")
            snap = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(snap["status"], "loaded")
            self.assertEqual(snap["mode"], "real")
            self.assertEqual(snap["phase"], "pre")
            self.assertTrue(snap["ok"])
            self.assertEqual(snap["db"]["schema_version"], 16,
                             "db.schema_version must record the real user_version")
            self.assertEqual(snap["db"]["schema_kind"], "loaded")
            self.assertEqual(snap["db"]["source_state"], "loaded")
            self.assertEqual(snap["db"]["selection_state"], "resolved")
            self.assertIs(snap["db"]["readonly_open"], True)
            self.assertIs(snap["db"]["query_only"], True)
            self.assertEqual(snap["db"]["writes_attempted"], 0)
            self.assertIs(snap["db"]["path_redacted"], True)
            self.assertEqual(snap["ledger"]["writes_attempted"], 0)
            self.assertIs(snap["redacted"], True)
            app_types = {u["app_type"] for u in snap["route"]["active_upstreams"]}
            self.assertEqual(app_types, {"claude", "codex", "gemini"})
            self.assertTrue(snap["route"]["identity_sha256"])

    def test_full_aba_roundtrip_schema16_passes_with_binding(self) -> None:
        """A genuine A->B->A round trip on a schema-16 database passes under
        the same fail-closed contract as schema 12."""
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = make_schema16_db(tmp, "v16.db")
            pre = tmp / "pre.json"
            b = tmp / "b.json"
            post = tmp / "post.json"
            proc = run_collect(db, "pre", pre)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            flip_to_b_schema16(db)
            proc = run_collect(db, "observed-b", b)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            flip_to_a_schema16(db)
            proc = run_collect(db, "post", post)
            self.assertEqual(proc.returncode, 0, proc.stderr)

            pre_snap = json.loads(pre.read_text(encoding="utf-8"))
            b_snap = json.loads(b.read_text(encoding="utf-8"))
            post_snap = json.loads(post.read_text(encoding="utf-8"))
            self.assertNotEqual(b_snap["route"]["identity_sha256"],
                                pre_snap["route"]["identity_sha256"],
                                "B state must differ from A for a genuine round trip")
            self.assertEqual(pre_snap["route"]["identity_sha256"],
                             post_snap["route"]["identity_sha256"])
            for snap in (pre_snap, b_snap, post_snap):
                self.assertEqual(snap["db"]["schema_version"], 16)
                self.assertEqual(snap["db"]["source_state"], "loaded")
                self.assertEqual(snap["db"]["writes_attempted"], 0)
            self.assertEqual(pre_snap["db"]["path_fingerprint_sha256"],
                             post_snap["db"]["path_fingerprint_sha256"])

            artifact = tmp / "cc-switch-aba-v16.json"
            proc = run_harness(
                "verify", "--pre", str(pre), "--observed-b", str(b), "--post", str(post),
                "--actions", str(make_actions(tmp)), "--db", str(db),
                "--expected-commit", repo_commit(), "--expected-source-digest", repo_digest(),
                "--artifact", str(artifact), "--quiet",
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["status"], "passed")
            self.assertIs(data["ok"], True)
            self.assertIs(data["wrote_cc_switch"], False)
            self.assertIs(data["identity_restored"], True)
            self.assertIs(data["observed_b_differs"], True)
            self.assertEqual(data["failure"]["class"], "none")
            self.assertEqual(data["db"]["schema_version"], 16)
            self.assertEqual(data["db"]["schema_kind"], "loaded")
            self.assertIs(data["db"]["db_unchanged_during_verify"], True)
            self.assertEqual(
                data["artifact_sha256"], recompute_artifact_sha256(data),
                "artifact_sha256 must equal SHA-256 of the canonical JSON without itself",
            )
            check = run_harness("--check-schema", str(artifact))
            self.assertEqual(check.returncode, 0, check.stderr)

    def test_schema16_endpoint_column_shape_fails_closed(self) -> None:
        """Schema-16 provider_endpoints stores url (not endpoint): a current
        provider without env config fails the per-provider parse (selection
        blocked) while the snapshot itself remains loaded — never a crash and
        never a fabricated identity."""
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = make_schema16_db(tmp, "v16.db")
            conn = sqlite3.connect(str(db))
            conn.execute("UPDATE providers SET is_current = 0 WHERE id = 'v16-codex-current'")
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, "
                "sort_index, is_current, meta) "
                "VALUES ('v16-endpoint-only', 'codex', 'endpoint-only', "
                "'{\"auth\":{\"auth_mode\":\"chatgpt\"}}', 0, 1, '{}')"
            )
            conn.commit()
            conn.close()
            out = tmp / "snap.json"
            proc = run_collect(db, "pre", out)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            snap = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(snap["status"], "loaded")
            self.assertEqual(snap["db"]["schema_version"], 16)
            self.assertEqual(snap["db"]["source_state"], "loaded")
            self.assertEqual(snap["db"]["selection_state"], "blocked")
            codex_ids = [u["provider_id"] for u in snap["route"]["active_upstreams"]
                         if u["app_type"] == "codex"]
            self.assertEqual(codex_ids, [],
                             "unparseable provider must not fabricate an upstream")
            self.assertEqual(snap["db"]["writes_attempted"], 0)

    def test_collect_unreadable_db_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = tmp / "garbage.db"
            db.write_bytes(b"not a sqlite database at all")
            out = tmp / "snap.json"
            proc = run_collect(db, "pre", out)
            self.assertEqual(proc.returncode, 1)
            snap = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(snap["status"], "unreadable")

    def test_collect_requires_explicit_db_in_real_mode(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            proc = run_harness("collect", "--out", str(Path(tmp) / "x.json"))
            self.assertEqual(proc.returncode, 2, "real mode must never auto-discover a DB path")

    def test_fixture_mode_refuses_real_db_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            proc = run_harness("collect", "--fixture", "--db", str(Path(tmp) / "real.db"),
                               "--out", str(Path(tmp) / "x.json"))
            self.assertEqual(proc.returncode, 2, "--fixture and --db are mutually exclusive")

    def test_full_aba_roundtrip_passes_with_binding(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre, b, post, db = collect_aba_phases(tmp)

            b_snap = json.loads(b.read_text(encoding="utf-8"))
            pre_snap = json.loads(pre.read_text(encoding="utf-8"))
            self.assertNotEqual(b_snap["route"]["identity_sha256"],
                                pre_snap["route"]["identity_sha256"],
                                "B state must differ from A for a genuine round trip")
            codex_ids = [u["provider_id"] for u in b_snap["route"]["active_upstreams"]
                         if u["app_type"] == "codex"]
            self.assertEqual(codex_ids, ["fixture-codex-alt"])
            # All three phases were collected from the same database path.
            self.assertEqual(pre_snap["db"]["path_fingerprint_sha256"],
                             b_snap["db"]["path_fingerprint_sha256"])
            post_snap = json.loads(post.read_text(encoding="utf-8"))
            self.assertEqual(pre_snap["db"]["path_fingerprint_sha256"],
                             post_snap["db"]["path_fingerprint_sha256"])

            artifact = tmp / "cc-switch-aba.json"
            proc = run_harness(
                "verify", "--pre", str(pre), "--observed-b", str(b), "--post", str(post),
                "--actions", str(make_actions(tmp)), "--db", str(db),
                "--expected-commit", repo_commit(), "--expected-source-digest", repo_digest(),
                "--artifact", str(artifact), "--quiet",
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["status"], "passed")
            self.assertIs(data["ok"], True)
            self.assertIs(data["wrote_cc_switch"], False)
            self.assertIs(data["identity_restored"], True)
            self.assertIs(data["observed_b_differs"], True)
            self.assertEqual(data["route"]["pre_identity_sha256"], data["route"]["post_identity_sha256"])
            self.assertEqual(data["failure"]["class"], "none")
            self.assertEqual(data["candidate"]["commit"], repo_commit())
            self.assertIs(data["candidate"]["resolved"], True)
            self.assertEqual(data["source_digest"]["value"], repo_digest())
            self.assertIs(data["source_digest"]["resolved"], True)
            self.assertEqual(data["source_digest"]["scope"], "n0-canonical-source-manifest")
            self.assertEqual(
                data["artifact_sha256"],
                recompute_artifact_sha256(data),
                "artifact_sha256 must equal SHA-256 of the canonical JSON without itself",
            )
            self.assertEqual(data["db"]["writes_attempted"], 0)
            self.assertIs(data["db"]["db_unchanged_during_verify"], True)
            self.assertEqual(len(data["ledger"]["computer_actions"]), 2)
            self.assertEqual(data["restore"]["status"], "no_changes_made")
            self.assertEqual(data["mode"], "real")

    def test_identity_not_restored_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre, b, _, db = collect_aba_phases(tmp)
            # Switch back to B (NOT A) and collect post from the same path.
            flip_to_b(db)
            post_b = tmp / "post-b.json"
            proc = run_collect(db, "post", post_b)
            self.assertEqual(proc.returncode, 0)
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(b),
                               "--post", str(post_b), "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 1)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["status"], "blocked")
            self.assertEqual(data["failure"]["class"], "identity_mismatch")
            self.assertIs(data["identity_restored"], False)

    def test_switch_not_observed_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = copy_fixture(tmp, "a.db")
            snap = tmp / "snap.json"
            run_collect(db, "pre", snap)
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(snap), "--observed-b", str(snap),
                               "--post", str(snap), "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 1)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["failure"]["class"], "switch_not_observed")
            self.assertIs(data["observed_b_differs"], False)

    def test_missing_or_blocked_observed_b_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre_db = copy_fixture(tmp, "a.db")
            pre = tmp / "pre.json"
            run_collect(pre_db, "pre", pre)
            # A blocked snapshot (unknown schema) as observed-b must fail closed.
            blocked = tmp / "blocked.json"
            db = tmp / "unknown.db"
            conn = sqlite3.connect(str(db))
            conn.execute("PRAGMA user_version = 7")
            conn.execute("CREATE TABLE providers (id TEXT PRIMARY KEY)")
            conn.commit()
            conn.close()
            proc = run_collect(db, "observed-b", blocked)
            self.assertEqual(proc.returncode, 1)
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(blocked),
                               "--post", str(pre), "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 1)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["failure"]["class"], "missing_observed_b")
            # A missing observed-b file is a usage error, never a pass.
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(tmp / "nope.json"),
                               "--post", str(pre), "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 2)

    def test_tampered_writes_attempted_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = copy_fixture(tmp, "a.db")
            snap = tmp / "snap.json"
            run_collect(db, "pre", snap)
            tampered = tmp / "tampered.json"
            data = json.loads(snap.read_text(encoding="utf-8"))
            data["db"]["writes_attempted"] = 1
            tampered.write_text(json.dumps(data), encoding="utf-8")
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(snap), "--observed-b", str(tampered),
                               "--post", str(snap), "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 1)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["failure"]["class"], "wrote_cc_switch")
            self.assertIs(data["wrote_cc_switch"], False)

    def test_db_changed_during_verify_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre, b, post, db = collect_aba_phases(tmp)
            with open(db, "ab") as handle:
                handle.write(b"TAMPERED-AFTER-COLLECT")
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(b),
                               "--post", str(post), "--db", str(db),
                               "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 1)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["status"], "failed")
            self.assertEqual(data["failure"]["class"], "db_changed_during_verify")
            self.assertIs(data["db"]["db_unchanged_during_verify"], False)

    def test_expected_commit_mismatch_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre, b, _, _ = collect_aba_phases(tmp)
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(b),
                               "--post", str(pre), "--expected-commit", "0" * 40,
                               "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 1)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["failure"]["class"], "commit_mismatch")

    def test_expected_digest_mismatch_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre, b, _, _ = collect_aba_phases(tmp)
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(b),
                               "--post", str(pre), "--expected-source-digest", "0" * 64,
                               "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 1)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["failure"]["class"], "digest_mismatch")

    def test_actions_with_secret_shape_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre, b, _, _ = collect_aba_phases(tmp)
            actions = tmp / "bad-actions.json"
            actions.write_text(json.dumps([
                {"sequence": 1, "action": "switch", "detail": "sk-abcdef0123456789abcdef"},
            ]), encoding="utf-8")
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(b),
                               "--post", str(pre), "--actions", str(actions),
                               "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 2, "secret-shaped action entries must be rejected")

    def test_phases_from_different_db_paths_block(self) -> None:
        # An A->B->A round trip is only evidence on a SINGLE database path:
        # a post snapshot collected from a different file must fail closed
        # even when the identities match.
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre, b, _, _ = collect_aba_phases(tmp)
            other = copy_fixture(tmp, "other-copy.db")  # same A content, different path
            post_other = tmp / "post-other.json"
            proc = run_collect(other, "post", post_other)
            self.assertEqual(proc.returncode, 0)
            artifact = tmp / "a.json"
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(b),
                               "--post", str(post_other), "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 1)
            data = json.loads(artifact.read_text(encoding="utf-8"))
            self.assertEqual(data["failure"]["class"], "db_path_mismatch",
                             "phases collected from different database paths must block")

    def test_check_schema_accepts_artifact_and_rejects_tampered(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pre, b, _, _ = collect_aba_phases(tmp)
            artifact = tmp / "cc-switch-aba.json"
            proc = run_harness("verify", "--pre", str(pre), "--observed-b", str(b),
                               "--post", str(pre), "--artifact", str(artifact), "--quiet")
            self.assertEqual(proc.returncode, 0)
            check = run_harness("--check-schema", str(artifact))
            self.assertEqual(check.returncode, 0, check.stderr)
            # Any tampering that would weaken the evidence contract must be rejected.
            data = json.loads(artifact.read_text(encoding="utf-8"))
            data["wrote_cc_switch"] = True
            tampered = tmp / "tampered.json"
            tampered.write_text(json.dumps(data), encoding="utf-8")
            check = run_harness("--check-schema", str(tampered))
            self.assertEqual(check.returncode, 1, "wrote_cc_switch=true must fail schema validation")
            # A snapshot may also be schema-checked.
            check = run_harness("--check-schema", str(pre))
            self.assertEqual(check.returncode, 0)

    def test_route_signatures_pin_fixture_identity(self) -> None:
        """The Python identity mirrors the d-token-config-patcher pinned format."""
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            db = copy_fixture(tmp, "a.db")
            out = tmp / "snap.json"
            run_collect(db, "pre", out)
            snap = json.loads(out.read_text(encoding="utf-8"))
            codex = next(u for u in snap["route"]["active_upstreams"] if u["app_type"] == "codex")
            expected_upstream = stable_hash(
                "cc-switch-upstream-v1\nfixture-codex-current\ncc-switch:codex\n"
                "open_ai\nhttps://fixture-codex.example.test/v1"
            )
            self.assertEqual(codex["upstream_signature"], expected_upstream,
                             "upstream signature must mirror the Rust pin")
            self.assertEqual(codex["route_signature"],
                             stable_hash(f"cc-switch-route-v1\n{expected_upstream}\ncodex-fixture-model"))
            self.assertEqual(codex["normalized_endpoint"], "https://fixture-codex.example.test/v1")
            # The identity hash must be deterministic over the same snapshot.
            snap2 = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(snap["route"]["identity_sha256"], snap2["route"]["identity_sha256"])

    def test_harness_source_never_writes_cc_switch(self) -> None:
        source = HARNESS.read_text(encoding="utf-8")
        self.assertIn("mode=ro", source, "harness must open the DB read-only via SQLite URI")
        self.assertIn("query_only", source, "harness must pin PRAGMA query_only")
        self.assertNotIn("mode=rw", source)
        self.assertNotIn("INSERT INTO", source)
        self.assertNotIn("UPDATE providers", source)
        self.assertNotIn("DELETE FROM", source)


if __name__ == "__main__":
    unittest.main()
