"""Behavior unit tests for the agent config acceptance runner (PR-6).

Covers: the seven-adapter whitelist, unsupported agents blocked, source
configs untouched and staged copies byte-exact, auth/secret filenames and
symlinks rejected, the candidate probe subprocess argv contract, public
booleans/hash verification, reverse-order exact rollback on failure,
evidence redaction, and fail-closed --dry-run / --check-schema modes.
The probe subprocess is always mocked; no cargo build ever runs.
"""

from __future__ import annotations

import gc
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
import warnings
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).parents[1] / "scripts" / "run_agent_config_acceptance.py"
SPEC = importlib.util.spec_from_file_location("run_agent_config_acceptance", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

ENDPOINT = "http://127.0.0.1:18001"
MODEL = "test-model"


def ok_report(copy_path: Path, agent_id: str) -> dict:
    """A probe report that satisfies verify_probe_report for the copy."""
    fingerprint = MODULE.hash_file(copy_path)
    return {
        "source": "dtoken-config-patcher",
        "agent_id": agent_id,
        "target": "~/.codex/config.toml",
        "configure": {
            "previewed": True,
            "revision_bound": True,
            "before_sha256": "a" * 64,
            "after_sha256": "b" * 64,
            "backup_created": True,
            "verified": True,
        },
        "clear": {
            "previewed": True,
            "revision_bound": True,
            "before_sha256": "b" * 64,
            "after_sha256": "c" * 64,
            "verified": True,
            "owned_transport_removed": True,
        },
        "restore": {
            "state": "restored_exact",
            "sha256": fingerprint,
            "mode_preserved": True,
        },
        "network_request_made": False,
        "agent_process_started": False,
        "raw_config_returned": False,
        "credential_value_read": False,
        "model_returned": False,
    }


def probe_result(copy_path: Path, agent_id: str) -> subprocess.CompletedProcess:
    report = ok_report(copy_path, agent_id)
    return subprocess.CompletedProcess(
        args=[], returncode=0, stdout=json.dumps(report) + "\n", stderr=""
    )


def expected_bindings() -> tuple[str, str]:
    """Independently compute the N9 bound-mode values for the worktree.

    Mirrors the runner's resolve_git_bindings: the HEAD commit and the
    sha256 of the HEAD tree id (`git rev-parse HEAD^{tree}` output without
    its trailing newline).
    """
    head = subprocess.check_output(
        ["git", "-C", str(MODULE.REPO_ROOT), "rev-parse", "HEAD"]
    ).decode("utf-8", errors="replace").strip()
    tree = subprocess.check_output(
        ["git", "-C", str(MODULE.REPO_ROOT), "rev-parse", "HEAD^{tree}"]
    )
    return head, MODULE.sha256_bytes(tree.removesuffix(b"\n"))


class AgentConfigAcceptanceTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.tempdir = Path(self._tmp.name)
        self.sources = self.tempdir / "sources"
        self.temp_root = self.tempdir / "staging"
        self.sources.mkdir()
        self.temp_root.mkdir()
        self.artifact = self.tempdir / "artifact.json"

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def write_config(self, name: str, content: bytes) -> Path:
        path = self.sources / name
        path.write_bytes(content)
        return path

    def run_cli(self, *args: str) -> tuple[int, str, str]:
        out, err = StringIO(), StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            try:
                code = MODULE.main(list(args))
            except SystemExit as exc:  # argparse exits on bad flags
                code = exc.code if isinstance(exc.code, int) else 2
        return code, out.getvalue(), err.getvalue()

    def staged_copy(self, index: int, name: str) -> Path:
        return self.temp_root / f"agent_{index}_{name}"

    def agent_args(self, *specs: str) -> list[str]:
        args: list[str] = []
        for spec in specs:
            args.append("--agent")
            args.append(spec)
        return args

    def ok_side_effect(self, calls: list) -> callable:
        def side_effect(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            calls.append((argv, kwargs))
            copy_path = Path(argv[8])
            agent_id = argv[7]
            return probe_result(copy_path, agent_id)

        return side_effect

    # --- whitelist + unsupported blocked ---

    def test_whitelist_contains_exactly_seven_adapters(self) -> None:
        self.assertEqual(
            MODULE.ADAPTERS,
            frozenset(
                {
                    "claude-code",
                    "codex",
                    "cline",
                    "gemini-cli",
                    "roo-code",
                    "continue",
                    "aider",
                }
            ),
        )

    def test_unsupported_agent_blocked(self) -> None:
        source = self.write_config("settings.json", b"{}")
        code, out, err = self.run_cli(
            *self.agent_args(f"unsupported-agent={source}"),
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("unsupported agent id", err)
        # a real execution writes a blocked structured artifact, not a bare exit
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(set(data), MODULE.ARTIFACT_KEYS)
        self.assertEqual(data["schema_version"], MODULE.SCHEMA_VERSION)
        self.assertEqual(data["status"], "blocked")
        self.assertEqual(data["mode"], "executed")
        self.assertEqual(data["restore_order"], [])
        # N9 bound mode: the blocked artifact is still bound to the
        # candidate revision
        head, digest = expected_bindings()
        self.assertEqual(data["candidate_commit"], head)
        self.assertEqual(data["source_digest"], digest)
        self.assertEqual(len(data["agents"]), 1)
        self.assertEqual(data["agents"][0]["agent_id"], "unsupported-agent")
        self.assertEqual(data["agents"][0]["status"], "failed")
        self.assertEqual(data["agents"][0]["error_class"], "UnsupportedAgent")
        self.assertEqual(data["agents"][0]["steps"], [])
        # blocked records carry real fingerprint/mode so the artifact stays
        # schema-valid
        self.assertEqual(
            data["agents"][0]["source_fingerprint"], MODULE.hash_file(source)
        )
        self.assertEqual(
            data["agents"][0]["source_mode"],
            oct(source.stat().st_mode & 0o777),
        )
        # nothing was staged or executed
        self.assertFalse((self.temp_root / "agent_0_settings.json").exists())

    def test_blocked_artifact_passes_schema(self) -> None:
        source = self.write_config("settings.json", b"{}")
        code, out, err = self.run_cli(
            *self.agent_args(f"unsupported-agent={source}"),
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 2, msg=out + err)
        violations = MODULE.check_artifact_schema(self.artifact)
        self.assertEqual(violations, [], msg=str(violations))
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "blocked")
        self.assertEqual(
            data["agents"][0]["source_fingerprint"], MODULE.hash_file(source)
        )

    def test_mixed_unsupported_blocked_records(self) -> None:
        supported = self.write_config("config.toml", b"[core]\n")
        bogus = self.write_config("settings.json", b"{}")
        code, out, err = self.run_cli(
            *self.agent_args(f"codex={supported}", f"bogus-agent={bogus}"),
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 2, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "blocked")
        by_id = {record["agent_id"]: record for record in data["agents"]}
        self.assertEqual(by_id["bogus-agent"]["error_class"], "UnsupportedAgent")
        self.assertEqual(by_id["codex"]["error_class"], "RunBlocked")
        self.assertEqual(by_id["codex"]["status"], "failed")
        self.assertEqual(
            by_id["codex"]["source_fingerprint"], MODULE.hash_file(supported)
        )
        self.assertEqual(MODULE.check_artifact_schema(self.artifact), [])

    def test_blocked_fingerprint_failure_no_artifact(self) -> None:
        source = self.write_config("settings.json", b"{}")

        def vanishing_read(path: Path) -> bytes:
            raise OSError("source vanished")

        with patch.object(MODULE, "read_no_follow", side_effect=vanishing_read):
            code, out, err = self.run_cli(
                *self.agent_args(f"bogus-agent={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("cannot fingerprint", err)
        self.assertFalse(self.artifact.exists())

    # --- N9 bound mode: candidate_commit + source_digest ---

    def test_git_bindings_resolved_and_bound(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        with patch.object(
            MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])
        ):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        head, digest = expected_bindings()
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(set(data), MODULE.ARTIFACT_KEYS)
        self.assertEqual(data["candidate_commit"], head)
        self.assertEqual(data["source_digest"], digest)
        self.assertEqual(MODULE.check_artifact_schema(self.artifact), [])
        # the binding is stable across two resolutions of the same tree
        self.assertEqual(
            MODULE.resolve_git_bindings(MODULE.REPO_ROOT), (head, digest)
        )

    def test_git_binding_unresolved_real_run_fail_closed(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")

        def unresolved(repo_root: Path) -> tuple[str, str]:
            raise MODULE.GitBindingUnresolved("cannot resolve git HEAD")

        with patch.object(MODULE, "resolve_git_bindings", side_effect=unresolved):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("cannot resolve git HEAD", err)
        self.assertFalse(
            self.artifact.exists(), "no artifact without a git binding"
        )

    def test_git_binding_unresolved_blocked_fail_closed(self) -> None:
        # the unsupported-adapter blocked path binds its artifact too; an
        # unresolved binding fails closed with no artifact, same as the run
        # path
        source = self.write_config("settings.json", b"{}")

        def unresolved(repo_root: Path) -> tuple[str, str]:
            raise MODULE.GitBindingUnresolved("cannot resolve git HEAD")

        with patch.object(MODULE, "resolve_git_bindings", side_effect=unresolved):
            code, out, err = self.run_cli(
                *self.agent_args(f"bogus-agent={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("cannot resolve git HEAD", err)
        self.assertFalse(
            self.artifact.exists(), "no blocked artifact without a git binding"
        )

    def test_check_schema_missing_bindings_rejected(self) -> None:
        # a five-key artifact without the bound-mode keys must fail closed
        bad = self.tempdir / "bad.json"
        bad.write_text(
            json.dumps(
                {
                    "schema_version": MODULE.SCHEMA_VERSION,
                    "status": "verified",
                    "mode": "executed",
                    "agents": [
                        {
                            "agent_id": "codex",
                            "adapter": "codex",
                            "source_fingerprint": "a" * 64,
                            "source_mode": "0o644",
                            "status": "ok",
                            "error_class": None,
                            "steps": [],
                        }
                    ],
                    "restore_order": [],
                }
            ),
            encoding="utf-8",
        )
        violations = MODULE.check_artifact_schema(bad)
        self.assertTrue(
            any("artifact missing keys" in v for v in violations), str(violations)
        )
        self.assertTrue(
            any("candidate_commit" in v for v in violations), str(violations)
        )
        self.assertTrue(
            any("source_digest" in v for v in violations), str(violations)
        )

    def test_check_schema_rejects_bad_bindings(self) -> None:
        bad = self.tempdir / "bad.json"
        bad.write_text(
            json.dumps(
                {
                    "schema_version": MODULE.SCHEMA_VERSION,
                    "status": "verified",
                    "mode": "executed",
                    "candidate_commit": "not-a-commit",
                    "source_digest": "short",
                    "agents": [
                        {
                            "agent_id": "codex",
                            "adapter": "codex",
                            "source_fingerprint": "a" * 64,
                            "source_mode": "0o644",
                            "status": "ok",
                            "error_class": None,
                            "steps": [],
                        }
                    ],
                    "restore_order": [],
                }
            ),
            encoding="utf-8",
        )
        violations = MODULE.check_artifact_schema(bad)
        self.assertTrue(
            any("candidate_commit must be" in v for v in violations),
            str(violations),
        )
        self.assertTrue(
            any("source_digest must be" in v for v in violations), str(violations)
        )

    # --- behavior: source unchanged + copies byte-exact ---

    def test_source_configs_untouched_and_copies_byte_exact(self) -> None:
        first = self.write_config("codex-config.json", b'{"provider":"x"}\n')
        second = self.write_config("claude-settings.json", b"key=value\n")
        before = [(path.read_bytes(), path.stat().st_mode) for path in (first, second)]
        calls: list = []
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect(calls)):
            code, out, err = self.run_cli(
                *self.agent_args(
                    f"codex={first}", f"claude-code={second}"
                ),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        after = [(path.read_bytes(), path.stat().st_mode) for path in (first, second)]
        self.assertEqual(before, after, "source configs must never be modified")
        # staged copies are byte-exact restores of the sources
        for index, name in ((0, "codex-config.json"), (1, "claude-settings.json")):
            copy_path = self.staged_copy(index, name)
            self.assertTrue(copy_path.is_file(), f"staged copy missing: {copy_path}")
            self.assertEqual(copy_path.read_bytes(), before[index][0])
        # artifact is the only output; a top-level object with exactly the
        # contract keys, and per-agent keys exactly the allowed set
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(set(data), MODULE.ARTIFACT_KEYS)
        self.assertEqual(data["schema_version"], MODULE.SCHEMA_VERSION)
        self.assertEqual(data["status"], "verified")
        self.assertEqual(data["mode"], "executed")
        self.assertEqual(data["restore_order"], [])
        head, digest = expected_bindings()
        self.assertEqual(data["candidate_commit"], head)
        self.assertEqual(data["source_digest"], digest)
        self.assertEqual(len(data["agents"]), 2)
        for record in data["agents"]:
            self.assertEqual(set(record), MODULE.ALLOWED_AGENT_KEYS)
            self.assertNotIn("restore_order", record)
            self.assertEqual(record["status"], "ok")
            step_names = [step["step"] for step in record["steps"]]
            self.assertEqual(step_names, ["copy", "probe", "verify"])
            self.assertTrue(all(step["status"] == "ok" for step in record["steps"]))

    # --- behavior: auth rejected ---

    def test_rejected_name_matching(self) -> None:
        for name in ("auth.json", "credentials.json", "api-key.txt", "token", "cookie"):
            self.assertTrue(MODULE.is_rejected_name(name), name)
        for name in ("settings.json", "config.toml", "providers.json"):
            self.assertFalse(MODULE.is_rejected_name(name), name)

    def test_auth_and_secret_files_rejected(self) -> None:
        auth = self.write_config("auth.json", b"{}")
        secret = self.write_config("credentials.json", b"{}")
        code, out, err = self.run_cli(
            *self.agent_args(f"codex={auth}", f"cline={secret}"),
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("rejected auth/secret file", err)
        # unsafe sources fail before any artifact is written
        self.assertFalse(self.artifact.exists(), "no artifact on rejected sources")

    def test_symlink_rejected(self) -> None:
        target = self.write_config("config.toml", b"[core]\n")
        link = self.sources / "config-link.toml"
        try:
            os.symlink(target, link)
        except OSError:
            self.skipTest("symlinks unsupported on this platform")
        code, out, err = self.run_cli(
            *self.agent_args(f"codex={link}"),
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("symlink config rejected", err)
        self.assertFalse(self.artifact.exists(), "no artifact on rejected symlink")

    def test_no_o_nofollow_fallback_rejects_symlink(self) -> None:
        # platforms without O_NOFOLLOW (Windows) must still reject a
        # symlink via the lstat+inode guard
        target = self.write_config("config.toml", b"[core]\n")
        link = self.sources / "config-link.toml"
        try:
            os.symlink(target, link)
        except OSError:
            self.skipTest("symlinks unsupported on this platform")
        with patch.object(MODULE, "_O_NOFOLLOW", 0):
            with self.assertRaises(OSError):
                MODULE.read_no_follow(link)

    def test_no_o_nofollow_fallback_rejects_symlink_source(self) -> None:
        target = self.write_config("config.toml", b"[core]\n")
        link = self.sources / "config-link.toml"
        try:
            os.symlink(target, link)
        except OSError:
            self.skipTest("symlinks unsupported on this platform")
        with patch.object(MODULE, "_O_NOFOLLOW", 0):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={link}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("symlink config rejected", err)
        self.assertFalse(self.artifact.exists())

    def test_no_o_nofollow_fallback_run_still_verified(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        with patch.object(MODULE, "_O_NOFOLLOW", 0):
            with patch.object(
                MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])
            ):
                code, out, err = self.run_cli(
                    *self.agent_args(f"codex={source}"),
                    "--temp-root",
                    str(self.temp_root),
                    "--artifact",
                    str(self.artifact),
                )
        self.assertEqual(code, 0, msg=out + err)

    # --- behavior: probe subprocess contract ---

    def test_probe_argv_contract(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        calls: list = []
        with patch.dict(
            os.environ,
            {"MY_API_KEY": "super-secret", "AWS_SESSION_TOKEN": "tok"},
            clear=False,
        ):
            with patch.object(
                MODULE, "_communicate_probe", side_effect=self.ok_side_effect(calls)
            ):
                code, out, err = self.run_cli(
                    *self.agent_args(f"codex={source}"),
                    "--temp-root",
                    str(self.temp_root),
                    "--artifact",
                    str(self.artifact),
                    "--endpoint",
                    ENDPOINT,
                    "--model",
                    MODEL,
                )
        self.assertEqual(code, 0, msg=out + err)
        self.assertEqual(len(calls), 1)
        argv, kwargs = calls[0]
        self.assertEqual(
            argv,
            [
                "cargo",
                "run",
                "-p",
                "dtoken-config-patcher",
                "--example",
                "agent_transport_candidate_probe",
                "--",
                "codex",
                str(self.staged_copy(0, "config.toml")),
                str(self.temp_root / "backups" / "agent_0"),
                ENDPOINT,
                MODEL,
            ],
        )
        self.assertEqual(set(kwargs), {"cwd", "env", "timeout"})
        self.assertEqual(kwargs["cwd"], str(MODULE.CARGO_DIR))
        self.assertIn("timeout", kwargs)
        env = kwargs["env"]
        self.assertEqual(env["HOME"], os.environ["HOME"])
        self.assertIn("PATH", env)
        self.assertNotIn("MY_API_KEY", env)
        self.assertNotIn("AWS_SESSION_TOKEN", env)

    def test_probe_env_scrubs_credentials(self) -> None:
        with patch.dict(
            os.environ,
            {
                "PATH": "/usr/bin:/bin",
                "HOME": "/home/probe",
                "TMP": "/tmp",
                "RUSTUP_HOME": "/rustup",
                "CARGO_HOME": "/cargo",
                "MY_API_KEY": "secret",
                "AWS_CREDENTIALS": "x",
                "COOKIE_JAR": "y",
                "OPENAI_PROVIDER_KEY": "z",
            },
            clear=True,
        ):
            env = MODULE.probe_env()
        self.assertEqual(env["PATH"], "/usr/bin:/bin")
        self.assertEqual(env["HOME"], "/home/probe")
        self.assertEqual(env["TMP"], "/tmp")
        self.assertEqual(env["RUSTUP_HOME"], "/rustup")
        self.assertEqual(env["CARGO_HOME"], "/cargo")
        self.assertNotIn("MY_API_KEY", env)
        self.assertNotIn("AWS_CREDENTIALS", env)
        self.assertNotIn("COOKIE_JAR", env)
        self.assertNotIn("OPENAI_PROVIDER_KEY", env)

    def test_probe_timeout_kills_whole_process_group(self) -> None:
        if not hasattr(os, "killpg"):
            self.skipTest("process-group kill unavailable on this platform")
        pid_file = self.tempdir / "child.pid"
        script = (
            "import subprocess, sys, time\n"
            f"subprocess.Popen([sys.executable, '-c', "
            f"'import os,time; open(\"{pid_file}\",\"w\").write(str(os.getpid())); "
            f"time.sleep(60)'])\n"
            "time.sleep(60)\n"
        )
        argv = [sys.executable, "-c", script]
        with self.assertRaises(subprocess.TimeoutExpired):
            MODULE._communicate_probe(
                argv, cwd=str(self.tempdir), env=dict(os.environ), timeout=5
            )
        # the grandchild must be dead too (group kill), not orphaned
        for _ in range(100):
            if pid_file.exists():
                break
            time.sleep(0.05)
        self.assertTrue(pid_file.exists(), "grandchild never started before timeout")
        child_pid = int(pid_file.read_text(encoding="utf-8"))
        deadline = time.time() + 5
        while time.time() < deadline:
            try:
                os.kill(child_pid, 0)
            except ProcessLookupError:
                break
            time.sleep(0.05)
        else:
            self.fail("probe grandchild survived the timeout group kill")

    def test_killpg_skipped_when_process_already_exited(self) -> None:
        # a reaped pid may have been recycled; killpg must only fire while
        # the direct child is still running (poll() is None)
        if not hasattr(os, "killpg"):
            self.skipTest("process-group kill unavailable on this platform")
        proc = subprocess.Popen(
            [sys.executable, "-c", "pass"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        proc.wait()
        with patch.object(
            os,
            "killpg",
            side_effect=AssertionError("killpg called on a reaped process"),
        ):
            MODULE._kill_process_group(proc)  # must return without calling killpg
        proc.stdout.close()
        proc.stderr.close()

    def test_communicate_timeout_leaks_no_resource_warning(self) -> None:
        # the timeout path abandons communicate()'s pipes; the Popen
        # context manager must close them, so GC never surfaces a
        # ResourceWarning about unclosed stdout/stderr
        if not hasattr(os, "killpg"):
            self.skipTest("process-group kill unavailable on this platform")
        argv = [sys.executable, "-c", "import time; time.sleep(60)\n"]
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            with self.assertRaises(subprocess.TimeoutExpired):
                MODULE._communicate_probe(
                    argv, cwd=str(self.tempdir), env=dict(os.environ), timeout=1
                )
            gc.collect()
        leaked = [
            str(w.message)
            for w in caught
            if issubclass(w.category, ResourceWarning)
        ]
        self.assertEqual(leaked, [], msg=f"unclosed probe pipes: {leaked}")

    def test_artifact_atomic_write_private_mode(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "verified")
        if os.name == "posix":
            self.assertEqual(self.artifact.stat().st_mode & 0o777, 0o600)
        self.assertFalse(
            self.artifact.with_name(f".{self.artifact.name}.tmp").exists()
        )

    def test_artifact_atomic_write_cleanup_on_failure(self) -> None:
        artifact_path = self.tempdir / "artifact.json"
        with patch.object(MODULE.json, "dumps", side_effect=ValueError("boom")):
            with self.assertRaises(ValueError):
                MODULE.write_artifact_atomic(artifact_path, {"schema_version": "1.0"})
        self.assertFalse(artifact_path.exists())
        leftovers = list(artifact_path.parent.glob(f".{artifact_path.name}.*.tmp"))
        self.assertEqual(leftovers, [], msg=f"leftover temp files: {leftovers}")

    def test_artifact_atomic_ignores_stale_random_tmp(self) -> None:
        # a stale temp from a crashed run under the old (or any) random
        # name must neither wedge the write nor be touched
        source = self.write_config("config.toml", b"[core]\n")
        stale = self.tempdir / f".{self.artifact.name}.stale1234.tmp"
        stale.write_text("stale", encoding="utf-8")
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "verified")
        self.assertEqual(stale.read_text(encoding="utf-8"), "stale")
        leftovers = list(self.tempdir.glob(f".{self.artifact.name}.*.tmp"))
        self.assertEqual(leftovers, [stale], msg=str(leftovers))

    def test_probe_failure_marks_record_failed(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")

        def failing_side_effect(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            return subprocess.CompletedProcess(
                args=argv, returncode=1, stdout="", stderr="boom"
            )

        with patch.object(MODULE, "_communicate_probe", side_effect=failing_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["mode"], "executed")
        self.assertEqual(data["agents"][0]["status"], "failed")
        self.assertEqual(data["agents"][0]["error_class"], "ProbeFailed")
        self.assertEqual(data["restore_order"], [])

    def test_probe_restore_hash_mismatch_failed(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")

        def mismatch_side_effect(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            report = ok_report(Path(argv[8]), argv[7])
            report["restore"]["sha256"] = "f" * 64
            return subprocess.CompletedProcess(
                args=argv, returncode=0, stdout=json.dumps(report) + "\n", stderr=""
            )

        with patch.object(MODULE, "_communicate_probe", side_effect=mismatch_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["agents"][0]["status"], "failed")
        self.assertEqual(data["agents"][0]["error_class"], "ProbeReportInvalid")
        self.assertEqual(data["restore_order"], [])

    def test_probe_public_boolean_violation_failed(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")

        def violation_side_effect(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            report = ok_report(Path(argv[8]), argv[7])
            report["network_request_made"] = True
            return subprocess.CompletedProcess(
                args=argv, returncode=0, stdout=json.dumps(report) + "\n", stderr=""
            )

        with patch.object(MODULE, "_communicate_probe", side_effect=violation_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["agents"][0]["status"], "failed")
        self.assertEqual(data["agents"][0]["error_class"], "ProbeReportInvalid")
        self.assertEqual(data["restore_order"], [])

    def test_copy_mode_change_detected_fail_closed(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        copy_path = self.staged_copy(0, "config.toml")

        def chmod_copy_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            # the probe changes the staged copy's permission bits but the
            # report still claims mode_preserved
            original_mode = source.stat().st_mode & 0o777
            os.chmod(copy_path, 0o600 if original_mode != 0o600 else 0o644)
            return probe_result(Path(argv[8]), argv[7])

        with patch.object(MODULE, "_communicate_probe", side_effect=chmod_copy_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["agents"][0]["error_class"], "ProbeReportInvalid")
        self.assertEqual(data["restore_order"], [])

    # --- behavior: reverse rollback on failure ---

    def test_reverse_rollback_on_failure(self) -> None:
        first = self.write_config("config-a.toml", b"[a]\n")
        second = self.write_config("config-b.toml", b"[b]\n")
        calls: list = []

        def rollback_side_effect(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            calls.append(argv)
            if len(calls) == 2:
                return subprocess.CompletedProcess(
                    args=argv, returncode=1, stdout="", stderr="boom"
                )
            return probe_result(Path(argv[8]), argv[7])

        with patch.object(MODULE, "_communicate_probe", side_effect=rollback_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={first}", f"cline={second}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        # first agent's staged copy exact-restored; top-level restore_order
        # records the agent id list in the actual reverse restore order
        first_copy = self.staged_copy(0, "config-a.toml")
        self.assertEqual(first_copy.read_bytes(), first.read_bytes())
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["mode"], "executed")
        self.assertEqual(data["restore_order"], ["codex"])
        agents = data["agents"]
        self.assertEqual(agents[0]["status"], "ok")
        self.assertNotIn("restore_order", agents[0])
        self.assertIn(
            {"step": "rollback_restore", "status": "ok"}, agents[0]["steps"]
        )
        self.assertEqual(agents[1]["status"], "failed")
        self.assertEqual(agents[1]["error_class"], "ProbeFailed")

    def test_source_hash_change_detected_fail_closed(self) -> None:
        first = self.write_config("config-a.toml", b"[a]\n")
        second = self.write_config("config-b.toml", b"[b]\n")
        calls: list = []

        def tamper_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            calls.append(argv)
            if len(calls) == 2:
                # an external actor modifies the original source after probe 2
                first.write_bytes(b"[a] tampered\n")
            return probe_result(Path(argv[8]), argv[7])

        with patch.object(MODULE, "_communicate_probe", side_effect=tamper_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={first}", f"cline={second}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        # every staged copy was exact-restored in the actual reverse order
        self.assertEqual(
            self.staged_copy(0, "config-a.toml").read_bytes(), b"[a]\n"
        )
        self.assertEqual(
            self.staged_copy(1, "config-b.toml").read_bytes(), b"[b]\n"
        )
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(set(data), MODULE.ARTIFACT_KEYS)
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["mode"], "executed")
        self.assertEqual(data["restore_order"], ["cline", "codex"])
        agents = data["agents"]
        self.assertEqual(agents[0]["status"], "failed")
        self.assertEqual(agents[0]["error_class"], "SourceModified")
        self.assertEqual(agents[1]["status"], "ok")
        for record in agents:
            self.assertIn(
                {"step": "rollback_restore", "status": "ok"}, record["steps"]
            )

    def test_source_mode_change_detected_fail_closed(self) -> None:
        first = self.write_config("config-a.toml", b"[a]\n")
        second = self.write_config("config-b.toml", b"[b]\n")
        original_mode = first.stat().st_mode & 0o777
        changed_mode = 0o600 if original_mode != 0o600 else 0o644
        calls: list = []

        def chmod_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            calls.append(argv)
            if len(calls) == 2:
                # permission mode changed while the run is in flight
                os.chmod(first, changed_mode)
            return probe_result(Path(argv[8]), argv[7])

        with patch.object(MODULE, "_communicate_probe", side_effect=chmod_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={first}", f"cline={second}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(set(data), MODULE.ARTIFACT_KEYS)
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["mode"], "executed")
        self.assertEqual(data["agents"][0]["error_class"], "SourceModified")
        self.assertEqual(data["restore_order"], ["cline", "codex"])
        self.assertEqual(
            self.staged_copy(0, "config-a.toml").read_bytes(), b"[a]\n"
        )

    def test_later_source_change_no_index_error(self) -> None:
        # a change to a not-yet-staged agent's source must fail closed
        # instead of crashing on a missing record
        first = self.write_config("config-a.toml", b"[a]\n")
        second = self.write_config("config-b.toml", b"[b]\n")
        third = self.write_config("config-c.toml", b"[c]\n")
        calls: list = []

        def late_tamper_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            calls.append(argv)
            if len(calls) == 1:
                second.write_bytes(b"[b] tampered\n")
            return probe_result(Path(argv[8]), argv[7])

        with patch.object(
            MODULE, "_communicate_probe", side_effect=late_tamper_side_effect
        ):
            code, out, err = self.run_cli(
                *self.agent_args(
                    f"codex={first}", f"cline={second}", f"aider={third}"
                ),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["mode"], "executed")
        self.assertEqual(data["agents"][0]["status"], "ok")
        self.assertEqual(data["agents"][1]["status"], "failed")
        self.assertEqual(data["agents"][1]["error_class"], "SourceModified")
        self.assertEqual(data["restore_order"], ["cline", "codex"])
        self.assertEqual(
            self.staged_copy(0, "config-a.toml").read_bytes(), b"[a]\n"
        )
        self.assertEqual(
            self.staged_copy(1, "config-b.toml").read_bytes(), b"[b]\n"
        )

    def test_copy_symlink_substitution_never_writes_outside(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        external = self.tempdir / "external-target.txt"
        external.write_bytes(b"attacker payload\n")
        copy_path = self.staged_copy(0, "config.toml")

        def symlink_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            # the probe replaces the staged copy with a symlink pointing
            # outside the temp root
            try:
                copy_path.unlink()
            except FileNotFoundError:
                pass
            os.symlink(external, copy_path)
            return subprocess.CompletedProcess(
                args=argv, returncode=1, stdout="", stderr="boom"
            )

        with patch.object(MODULE, "_communicate_probe", side_effect=symlink_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        # the restore never wrote through the symlink
        self.assertEqual(external.read_bytes(), b"attacker payload\n")
        # the staged copy is a fresh regular file, byte-exact restored
        self.assertFalse(copy_path.is_symlink())
        self.assertEqual(copy_path.read_bytes(), b"[core]\n")
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["agents"][0]["error_class"], "ProbeFailed")

    def test_preseeded_hard_link_slot_never_written_through(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        external = self.tempdir / "external-target.txt"
        external.write_bytes(b"attacker payload\n")
        copy_path = self.staged_copy(0, "config.toml")
        # attacker pre-seeds the staging slot with a hard link to an
        # external same-filesystem file before the run starts
        os.link(external, copy_path)
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        self.assertEqual(external.read_bytes(), b"attacker payload\n")
        self.assertEqual(copy_path.read_bytes(), b"[core]\n")
        self.assertNotEqual(
            (copy_path.lstat().st_dev, copy_path.lstat().st_ino),
            (external.lstat().st_dev, external.lstat().st_ino),
        )

    def test_hard_link_substitution_restore_never_writes_through(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        external = self.tempdir / "external-target.txt"
        external.write_bytes(b"attacker payload\n")
        copy_path = self.staged_copy(0, "config.toml")

        def hardlink_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            # the probe replaces the staged copy with a hard link to an
            # external same-filesystem file, then fails
            try:
                copy_path.unlink()
            except FileNotFoundError:
                pass
            os.link(external, copy_path)
            return subprocess.CompletedProcess(
                args=argv, returncode=1, stdout="", stderr="boom"
            )

        with patch.object(MODULE, "_communicate_probe", side_effect=hardlink_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        # the restore unlinked the hard link instead of writing through it
        self.assertEqual(external.read_bytes(), b"attacker payload\n")
        self.assertFalse(copy_path.is_symlink())
        self.assertEqual(copy_path.read_bytes(), b"[core]\n")
        self.assertNotEqual(
            (copy_path.lstat().st_dev, copy_path.lstat().st_ino),
            (external.lstat().st_dev, external.lstat().st_ino),
        )

    def test_byte_identical_hard_link_substitution_detected(self) -> None:
        # the probe replaces the staged copy with a hard link to the real
        # config; bytes and mode are identical, so a byte-only check would
        # pass, but the inode check must fail closed
        source = self.write_config("config.toml", b"[core]\n")
        copy_path = self.staged_copy(0, "config.toml")

        def link_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            try:
                copy_path.unlink()
            except FileNotFoundError:
                pass
            os.link(source, copy_path)
            return probe_result(copy_path, argv[7])

        with patch.object(MODULE, "_communicate_probe", side_effect=link_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["agents"][0]["error_class"], "ProbeReportInvalid")
        self.assertEqual(data["status"], "failed")
        self.assertEqual(source.read_bytes(), b"[core]\n")
        # rollback detached the hard link and re-created a fresh file
        self.assertNotEqual(
            (copy_path.lstat().st_dev, copy_path.lstat().st_ino),
            (source.lstat().st_dev, source.lstat().st_ino),
        )
        self.assertEqual(copy_path.read_bytes(), b"[core]\n")

    def test_preseeded_source_hard_link_slot_fails_closed(self) -> None:
        # temp root is the sources directory and the staging slot is
        # pre-seeded as a hard link to the source itself: the runner must
        # fail closed BEFORE any unlink/write, leaving the source inode,
        # bytes and mode untouched, and must never run a probe.
        self.temp_root = self.sources
        source = self.temp_root / "config.toml"
        source.write_bytes(b"[core]\n")
        before = (source.stat().st_dev, source.stat().st_ino)
        slot = self.staged_copy(0, "config.toml")
        os.link(source, slot)
        calls: list = []
        with patch.object(
            MODULE, "_communicate_probe", side_effect=self.ok_side_effect(calls)
        ):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        self.assertIn("ConfigNotFile", out)
        self.assertEqual(calls, [], "colliding slot must never run a probe")
        self.assertEqual(source.read_bytes(), b"[core]\n")
        self.assertEqual(
            (source.stat().st_dev, source.stat().st_ino), before
        )
        # the slot was never unlinked or rewritten
        self.assertEqual((slot.lstat().st_dev, slot.lstat().st_ino), before)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "failed")
        self.assertEqual(data["agents"][0]["error_class"], "ConfigNotFile")
        self.assertEqual(data["restore_order"], [])

    # --- evidence redaction ---

    def test_evidence_redaction(self) -> None:
        source = self.write_config("config.toml", b"[core]\nsecret\n")
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        text = json.dumps(data)
        self.assertNotIn("secret", text)
        self.assertNotIn("~/.codex", text)  # probe target path never emitted
        self.assertNotIn(str(self.sources), text)
        for location, value in MODULE.iter_strings(data):
            self.assertFalse(MODULE.is_absolute_like(value), location)
        self.assertEqual(set(data), MODULE.ARTIFACT_KEYS)
        self.assertEqual(set(data["agents"][0]), MODULE.ALLOWED_AGENT_KEYS)

    def test_absolute_like_unc_and_drive_relative(self) -> None:
        self.assertTrue(MODULE.is_absolute_like("\\\\host\\share\\file"))
        self.assertTrue(MODULE.is_absolute_like("//host/share/file"))
        self.assertTrue(MODULE.is_absolute_like("C:\\abs\\file.txt"))
        self.assertTrue(MODULE.is_absolute_like("C:relative.txt"))
        self.assertTrue(MODULE.is_absolute_like("/abs/path"))
        self.assertFalse(MODULE.is_absolute_like("relative/path.txt"))
        self.assertFalse(MODULE.is_absolute_like("~/.codex/config.toml"))
        self.assertFalse(MODULE.is_absolute_like("http://127.0.0.1:18001"))

    def test_schema_flags_unc_leak(self) -> None:
        leak = self.tempdir / "leak.json"
        leak.write_text(
            json.dumps(
                {
                    "schema_version": "1.0",
                    "status": "verified",
                    "mode": "executed",
                    "agents": [
                        {
                            "agent_id": "codex",
                            "adapter": "codex",
                            "source_fingerprint": "a" * 64,
                            "source_mode": "\\\\host\\share\\path",
                            "status": "ok",
                            "error_class": None,
                            "steps": [],
                        }
                    ],
                    "restore_order": [],
                }
            ),
            encoding="utf-8",
        )
        violations = MODULE.check_artifact_schema(leak)
        self.assertTrue(
            any("absolute path leaked" in v for v in violations), str(violations)
        )

    def test_check_schema_missing_agents_no_crash(self) -> None:
        # agents missing while restore_order is a list must yield clean
        # violations, not a TypeError from iterating None
        bad = self.tempdir / "bad.json"
        bad.write_text(
            json.dumps(
                {
                    "schema_version": "1.0",
                    "status": "verified",
                    "mode": "executed",
                    "restore_order": ["codex"],
                }
            ),
            encoding="utf-8",
        )
        violations = MODULE.check_artifact_schema(bad)
        self.assertTrue(any("agents must be" in v for v in violations), str(violations))
        self.assertTrue(
            any("restore_order[0]" in v for v in violations), str(violations)
        )

    def test_check_schema_agents_object_no_crash(self) -> None:
        # agents as a non-list object must yield clean violations, not an
        # AttributeError from .get() on a dict key
        bad = self.tempdir / "bad.json"
        bad.write_text(
            json.dumps(
                {
                    "schema_version": "1.0",
                    "status": "verified",
                    "mode": "executed",
                    "agents": {"codex": {}},
                    "restore_order": [],
                }
            ),
            encoding="utf-8",
        )
        violations = MODULE.check_artifact_schema(bad)
        self.assertTrue(any("agents must be" in v for v in violations), str(violations))

    def test_check_schema_duplicate_restore_order_rejected(self) -> None:
        # a restore_order repeating an agent id must yield a clean
        # violation, not silently pass the schema check
        bad = self.tempdir / "bad.json"
        bad.write_text(
            json.dumps(
                {
                    "schema_version": "1.0",
                    "status": "failed",
                    "mode": "executed",
                    "agents": [
                        {
                            "agent_id": "codex",
                            "adapter": "codex",
                            "source_fingerprint": "a" * 64,
                            "source_mode": "0o644",
                            "status": "failed",
                            "error_class": "ProbeFailed",
                            "steps": [],
                        }
                    ],
                    "restore_order": ["codex", "codex"],
                }
            ),
            encoding="utf-8",
        )
        violations = MODULE.check_artifact_schema(bad)
        self.assertTrue(
            any("duplicate agent id" in v for v in violations), str(violations)
        )

    # --- missing agent blocked ---

    def test_missing_agent_blocked(self) -> None:
        code, out, err = self.run_cli(
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("--agent", err)

    def test_duplicate_agent_spec_rejected_before_run(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        calls: list = []
        with patch.object(
            MODULE, "_communicate_probe", side_effect=self.ok_side_effect(calls)
        ):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}", f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("duplicate agent id: codex", err)
        self.assertEqual(calls, [], "duplicate spec must never run a probe")
        self.assertFalse(self.artifact.exists(), "no artifact on duplicate specs")

    def test_snapshot_failure_clean_exit_no_artifact(self) -> None:
        # a source that vanishes between validation and the pre-run
        # snapshot must fail closed with a clean exit, not a traceback, and
        # never write a fingerprint-less artifact
        source = self.write_config("config.toml", b"[core]\n")

        def vanishing_read(path: Path) -> bytes:
            raise OSError("source vanished")

        with patch.object(MODULE, "read_no_follow", side_effect=vanishing_read):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("cannot read config", err)
        self.assertFalse(self.artifact.exists())

    def test_source_deleted_before_lstat_no_artifact(self) -> None:
        # agent 0's probe has already run when agent 1's source is deleted
        # before its own lstat: the record can carry no fingerprint, so the
        # run must fail closed with NO artifact (writing a null-fingerprint
        # artifact would violate the tool's own schema), and agent 0's
        # staged copy must still be exact-restored first.
        first = self.write_config("config-a.toml", b"[a]\n")
        second = self.write_config("config-b.toml", b"[b]\n")
        calls: list = []

        def delete_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            calls.append(argv)
            if len(calls) == 1:
                second.unlink()
            return probe_result(Path(argv[8]), argv[7])

        with patch.object(MODULE, "_communicate_probe", side_effect=delete_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={first}", f"cline={second}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("cannot fingerprint", err)
        self.assertFalse(self.artifact.exists(), "no artifact with null fingerprint")
        self.assertEqual(
            self.staged_copy(0, "config-a.toml").read_bytes(), b"[a]\n"
        )

    def test_source_replaced_by_symlink_before_lstat_no_artifact(self) -> None:
        # agent 0's probe has already run when agent 1's source is replaced
        # by a symlink before its own lstat: same fail-closed guarantee as
        # the deletion case, with no artifact and no writes through the link.
        first = self.write_config("config-a.toml", b"[a]\n")
        second = self.write_config("config-b.toml", b"[b]\n")
        target = self.write_config("config-target.toml", b"[t]\n")
        calls: list = []

        def symlink_side_effect(
            argv: list[str], **kwargs: object
        ) -> subprocess.CompletedProcess:
            calls.append(argv)
            if len(calls) == 1:
                second.unlink()
                os.symlink(target, second)
            return probe_result(Path(argv[8]), argv[7])

        with patch.object(MODULE, "_communicate_probe", side_effect=symlink_side_effect):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={first}", f"cline={second}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("cannot fingerprint", err)
        self.assertFalse(self.artifact.exists(), "no artifact with null fingerprint")
        self.assertEqual(target.read_bytes(), b"[t]\n")
        self.assertEqual(
            self.staged_copy(0, "config-a.toml").read_bytes(), b"[a]\n"
        )

    def test_artifact_write_oserror_clean_exit(self) -> None:
        # artifact path blocked by an existing directory: clean stderr
        # exit 1, no traceback
        source = self.write_config("config.toml", b"[core]\n")
        (self.tempdir / "artifact.json").mkdir()
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        self.assertIn("cannot write artifact", err)
        self.assertNotIn("Traceback", err)

    def test_blocked_artifact_write_oserror_clean_exit(self) -> None:
        source = self.write_config("settings.json", b"{}")
        with patch.object(
            MODULE,
            "write_artifact_atomic",
            side_effect=OSError("disk full"),
        ):
            code, out, err = self.run_cli(
                *self.agent_args(f"bogus-agent={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 1, msg=out + err)
        self.assertIn("cannot write artifact", err)
        self.assertNotIn("Traceback", err)
        self.assertFalse(self.artifact.exists())

    # --- fail-closed dry-run / check-schema ---

    def test_dry_run_fail_closed(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        calls: list = []
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect(calls)):
            code, out, err = self.run_cli(
                "--dry-run",
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        self.assertIn("nothing executed", out)
        self.assertEqual(calls, [], "dry-run must never execute the probe")
        self.assertFalse(self.artifact.exists(), "dry-run must not write an artifact")

    def test_dry_run_rejects_invalid_inputs(self) -> None:
        source = self.write_config("auth.json", b"{}")
        code, out, err = self.run_cli(
            "--dry-run",
            *self.agent_args(f"codex={source}"),
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("rejected auth/secret file", err)

    def test_check_schema_fail_closed(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        # valid artifact passes schema check
        code, out, err = self.run_cli(
            "--check-schema",
            "--agent",
            "codex=unused",
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 0, msg=out + err)
        self.assertIn("schema valid", out)
        # tampered artifact fails closed
        self.artifact.write_text(
            json.dumps([{"agent_id": "codex", "leaked_key": True}]), encoding="utf-8"
        )
        code, out, err = self.run_cli(
            "--check-schema",
            "--agent",
            "codex=unused",
            "--temp-root",
            str(self.temp_root),
            "--artifact",
            str(self.artifact),
        )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("schema violation", err)

    def test_check_schema_requires_no_agent_or_temp_root(self) -> None:
        source = self.write_config("config.toml", b"[core]\n")
        with patch.object(MODULE, "_communicate_probe", side_effect=self.ok_side_effect([])):
            code, out, err = self.run_cli(
                *self.agent_args(f"codex={source}"),
                "--temp-root",
                str(self.temp_root),
                "--artifact",
                str(self.artifact),
            )
        self.assertEqual(code, 0, msg=out + err)
        # --check-schema needs neither --agent nor --temp-root
        code, out, err = self.run_cli(
            "--check-schema", "--artifact", str(self.artifact)
        )
        self.assertEqual(code, 0, msg=out + err)
        self.assertIn("schema valid", out)
        # tampered artifact still fails closed without --agent
        self.artifact.write_text(
            json.dumps([{"agent_id": "codex", "leaked_key": True}]),
            encoding="utf-8",
        )
        code, out, err = self.run_cli(
            "--check-schema", "--artifact", str(self.artifact)
        )
        self.assertEqual(code, 2, msg=out + err)
        self.assertIn("schema violation", err)


if __name__ == "__main__":
    unittest.main()
