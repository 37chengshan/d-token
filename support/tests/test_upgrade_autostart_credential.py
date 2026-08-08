"""Behavior unit tests for the PR-8 producers (PLATFORM-1/CONFIG-4).

Covers the macOS upgrade/rollback audit runner and the autostart/credential
round-trip runner: dry-run default with zero side effects, explicit
--execute gating, fail-closed refusal of existing same-name entries,
platform gates (blocked, never fabricated), post-create verification
failure -> ok=false with cleanup, delete/restore failure -> ok=false,
rollback digest mismatch -> ok=false, artifact schema validation, and
redaction (no secrets, no file contents, no absolute paths).

All subprocess execution is mocked; no real DMG, launchctl, security, reg
or cmdkey command ever runs, and no user configuration or credential store
is touched. Every test uses a unique namespace and temp fixtures.
"""

from __future__ import annotations

import importlib.util
import json
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

SUPPORT = Path(__file__).resolve().parents[1]

AUDIT_SCRIPT = SUPPORT / "scripts" / "run_macos_upgrade_rollback_audit.py"
ROUNDTRIP_SCRIPT = SUPPORT / "scripts" / "run_autostart_credential_roundtrip.py"


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


AUDIT = load_module("run_macos_upgrade_rollback_audit", AUDIT_SCRIPT)
ROUND = load_module("run_autostart_credential_roundtrip", ROUNDTRIP_SCRIPT)

COMMIT = "c5a584971a9e2421a0040eb0f777ba76d9ae8f88"
DIGEST = "ab" * 32
SECRET = "0123456789abcdef" * 2  # 32 hex chars, matches token_hex(16) shape


def cp(returncode: int, stdout: str = "", stderr: str = "") -> subprocess.CompletedProcess:
    return subprocess.CompletedProcess(
        args=[], returncode=returncode, stdout=stdout, stderr=stderr
    )


def run_main(module, argv: list[str]) -> tuple[int, str, str]:
    out, err = StringIO(), StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        code = module.main(argv)
    return code, out.getvalue(), err.getvalue()


@contextmanager
def mac_host():
    """Force the audit script's darwin execute path regardless of the host
    platform. The lifecycle is fully offline (mocked subprocess), so the
    darwin-only branch is exercised deterministically on any CI host."""
    with patch.object(AUDIT.sys, "platform", "darwin"):
        yield


@contextmanager
def darwin_host():
    """Force the round-trip script's darwin execute path regardless of the
    host platform, with a fixed uid for the launchctl GUI domain. The
    lifecycle is fully offline (mocked subprocess)."""
    with patch.object(ROUND.sys, "platform", "darwin"), \
            patch.object(ROUND.os, "getuid", lambda: 501, create=True):
        yield


def make_bundle(root: Path, app_name: str, version: str, exe_name: str, payload: str) -> Path:
    bundle = root / app_name
    contents = bundle / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir(parents=True)
    info = {
        "CFBundleIdentifier": "com.example.dtoken",
        "CFBundleShortVersionString": version,
        "CFBundleExecutable": exe_name,
    }
    (contents / "Info.plist").write_bytes(plistlib.dumps(info))
    (contents / "MacOS" / exe_name).write_bytes(payload.encode("utf-8"))
    (contents / "Resources" / "data.bin").write_bytes(b"x" * 64)
    return bundle


class AuditMocks:
    """Stateful _run mock that simulates hdiutil attach/detach + lsregister."""

    def __init__(self, install_root: Path, upgrade_root: Path, app_name: str,
                 bundle_id: str = "com.example.dtoken") -> None:
        self.install_root = install_root
        self.upgrade_root = upgrade_root
        self.app_name = app_name
        self.bundle_id = bundle_id
        self.calls: list[list[str]] = []
        self.detach_fail_on: list[str] = []

    def __call__(self, argv: list[str], **kwargs):
        self.calls.append(argv)
        if argv[0] == "hdiutil":
            if argv[1] == "attach":
                mountpoint = Path(argv[argv.index("-mountpoint") + 1])
                source = self.install_root if argv[-1].endswith("install") else self.upgrade_root
                if argv[-1].endswith("install.dmg"):
                    source = self.install_root
                if not mountpoint.is_dir():
                    mountpoint.mkdir(parents=True)
                bundle = source / self.app_name
                target = mountpoint / self.app_name
                if target.exists():
                    shutil.rmtree(target)
                shutil.copytree(bundle, target)
                return cp(0)
            if argv[1] == "detach":
                if Path(argv[2]).name in self.detach_fail_on:
                    return cp(1, stderr="detach failed")
                return cp(0)
            return cp(1, stderr="unknown hdiutil")
        if argv[0].endswith("lsregister"):
            dump = (
                f"bundle id:            {self.bundle_id}\n"
                f"path:                 /Applications/{self.app_name}\n"
            )
            return cp(0, stdout=dump)
        return cp(0)


class MacosUpgradeRollbackAuditTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.app_dir = self.root / "apps"
        self.app_dir.mkdir()
        self.install_dmg = self.root / "install.dmg"
        self.upgrade_dmg = self.root / "upgrade.dmg"
        self.install_dmg.write_bytes(b"install-dmg-bytes")
        self.upgrade_dmg.write_bytes(b"upgrade-dmg-bytes")
        self.mount_install = self.root / "mnt-install"
        self.mount_upgrade = self.root / "mnt-upgrade"
        make_bundle(self.mount_install, "Fixture.app", "1.0.0", "fixture", "install-exe")
        make_bundle(self.mount_upgrade, "Fixture.app", "2.0.0", "fixture", "upgrade-exe")
        self.artifact = self.root / "artifact.json"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def base_args(self, execute: bool = True) -> list[str]:
        args = [
            "--install-dmg", str(self.install_dmg),
            "--upgrade-dmg", str(self.upgrade_dmg),
            "--app-dir", str(self.app_dir),
            "--app-name", "Fixture.app",
            "--namespace", "audit-ns",
            "--candidate-commit", COMMIT,
            "--source-digest", DIGEST,
            "--artifact", str(self.artifact),
        ]
        if execute:
            args.append("--execute")
        return args

    def test_no_args_is_validation_failure_with_no_side_effects(self) -> None:
        with patch.object(AUDIT, "_run") as mock_run:
            code, _out, err = run_main(AUDIT, [])
        self.assertEqual(code, 2)
        self.assertIn("required", err)
        mock_run.assert_not_called()
        self.assertFalse(self.artifact.exists())

    def test_default_mode_is_dry_run_with_no_side_effects(self) -> None:
        with patch.object(AUDIT, "_run") as mock_run:
            code, out, _err = run_main(AUDIT, self.base_args(execute=False))
        self.assertEqual(code, 0)
        self.assertIn("dry-run: inputs valid", out)
        self.assertIn("install", out)
        self.assertIn("rollback", out)
        mock_run.assert_not_called()
        self.assertFalse(self.artifact.exists())

    def test_explicit_dry_run_flag_is_equivalent(self) -> None:
        args = self.base_args(execute=False) + ["--dry-run"]
        with patch.object(AUDIT, "_run") as mock_run:
            code, _out, _err = run_main(AUDIT, args)
        self.assertEqual(code, 0)
        mock_run.assert_not_called()

    def test_dry_run_rejects_existing_same_name_target(self) -> None:
        make_bundle(self.app_dir, "Fixture.app", "9.9.9", "fixture", "user-install")
        with patch.object(AUDIT, "_run") as mock_run:
            code, _out, err = run_main(AUDIT, self.base_args(execute=False))
        self.assertEqual(code, 2)
        self.assertIn("refusing existing same-name", err)
        mock_run.assert_not_called()
        # The user's bundle is untouched.
        self.assertTrue((self.app_dir / "Fixture.app").exists())

    def test_execute_without_binding_is_rejected(self) -> None:
        args = self.base_args(execute=True)
        args.remove("--candidate-commit")
        args.remove(COMMIT)
        args.remove("--source-digest")
        args.remove(DIGEST)
        with patch.object(AUDIT, "_run") as mock_run:
            code, _out, err = run_main(AUDIT, args)
        self.assertEqual(code, 2)
        self.assertIn("candidate-commit", err)
        mock_run.assert_not_called()

    def test_execute_on_non_darwin_writes_blocked_artifact(self) -> None:
        with patch.object(AUDIT, "_run") as mock_run, \
                patch.object(AUDIT.sys, "platform", "win32"):
            code, _out, err = run_main(AUDIT, self.base_args(execute=True))
        self.assertEqual(code, 1)
        self.assertIn("blocked", err)
        mock_run.assert_not_called()
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(artifact["status"], "blocked")
        self.assertFalse(artifact["ok"])
        self.assertIn("platform-unsupported", artifact["errors"][0])
        # Blocked runs never mounted/copied/removed anything: no false claims.
        self.assertIsNone(artifact["rollback"]["matched_install_digest"])
        self.assertIsNone(artifact["rollback"]["version_equal_install"])
        self.assertIsNone(artifact["cleanup"]["mounts_detached"])
        self.assertIsNone(artifact["cleanup"]["app_removed"])
        self.assertIsNone(artifact["cleanup"]["restored_exact"])
        self.assertIsNone(artifact["cleanup"]["temp_root_removed"])

    def test_happy_path_verified_with_hash_mode_evidence(self) -> None:
        mocks = AuditMocks(self.mount_install, self.mount_upgrade, "Fixture.app")
        with mac_host(), patch.object(AUDIT, "_run", mocks):
            code, out, _err = run_main(AUDIT, self.base_args(execute=True))
        self.assertEqual(code, 0, msg=out)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(artifact["status"], "verified")
        self.assertTrue(artifact["ok"])
        self.assertEqual(artifact["candidate_commit"], COMMIT)
        self.assertEqual(artifact["source_digest"], DIGEST)
        step_names = [step["step"] for step in artifact["steps"]]
        self.assertEqual(
            step_names,
            [
                "pre_state_snapshot", "install", "verify_installed",
                "upgrade", "verify_after_upgrade", "rollback",
                "verify_after_rollback", "uninstall", "verify_after_uninstall",
                "cleanup_restore",
            ],
        )
        self.assertTrue(all(step["status"] == "ok" for step in artifact["steps"]))
        install_step = artifact["steps"][2]
        upgrade_step = artifact["steps"][4]
        self.assertEqual(install_step["app_version"], "1.0.0")
        self.assertEqual(upgrade_step["app_version"], "2.0.0")
        self.assertNotEqual(install_step["digest"], upgrade_step["digest"])
        rollback_step = artifact["steps"][6]
        self.assertEqual(rollback_step["digest"], install_step["digest"])
        self.assertTrue(artifact["rollback"]["matched_install_digest"])
        # Rollback restores the install bundle, so the version equals the
        # install version while differing from the upgraded version.
        self.assertTrue(artifact["rollback"]["version_equal_install"])
        self.assertTrue(all(artifact["cleanup"].values()))
        self.assertEqual(artifact["errors"], [])
        # Sources are fingerprinted by name/hash/size.
        self.assertEqual(len(artifact["sources"]), 2)
        self.assertEqual(
            artifact["sources"][0]["sha256"],
            AUDIT.hash_file(self.install_dmg),
        )
        # App dir restored to empty pre-state.
        self.assertFalse((self.app_dir / "Fixture.app").exists())

    def test_artifact_contains_no_absolute_paths_or_contents(self) -> None:
        mocks = AuditMocks(self.mount_install, self.mount_upgrade, "Fixture.app")
        with mac_host(), patch.object(AUDIT, "_run", mocks):
            run_main(AUDIT, self.base_args(execute=True))
        text = self.artifact.read_text(encoding="utf-8")
        self.assertNotIn(self.tmp.name, text)
        self.assertNotIn("install-exe", text)
        self.assertNotIn("upgrade-exe", text)
        self.assertNotIn("install-dmg-bytes", text)
        # Step digests are hex only (empty for non-bundle steps); no file
        # contents anywhere.
        digest_steps = {
            "verify_installed", "verify_after_upgrade", "verify_after_rollback",
        }
        for step in json.loads(text)["steps"]:
            if step["step"] in digest_steps:
                self.assertRegex(step["digest"], r"^[0-9a-f]{64}$")
                self.assertRegex(step["info_plist_sha256"], r"^[0-9a-f]{64}$")
                self.assertRegex(step["executable_sha256"], r"^[0-9a-f]{64}$")
            else:
                self.assertEqual(step["digest"], "")

    def test_post_create_verification_failure_ok_false_with_cleanup(self) -> None:
        mocks = AuditMocks(self.mount_install, self.mount_upgrade, "Fixture.app")
        real_metadata = AUDIT.bundle_metadata
        calls = {"n": 0}

        def failing_metadata(bundle: Path):
            calls["n"] += 1
            if calls["n"] == 2:  # 1 = pre-state source read, 2 = verify_installed
                raise AUDIT.AuditError("Info.plist unreadable after install")
            return real_metadata(bundle)

        with mac_host(), patch.object(AUDIT, "_run", mocks), \
                patch.object(AUDIT, "bundle_metadata", failing_metadata):
            code, _out, _err = run_main(AUDIT, self.base_args(execute=True))
        self.assertEqual(code, 1)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(artifact["status"], "failed")
        self.assertFalse(artifact["ok"])
        self.assertTrue(any(
            step["step"] == "verify_installed" and step["status"] == "failed"
            for step in artifact["steps"]
        ))
        # Cleanup still ran: mounts detached, app removed, temp root removed.
        self.assertTrue(artifact["cleanup"]["mounts_detached"])
        self.assertTrue(artifact["cleanup"]["app_removed"])
        self.assertTrue(artifact["cleanup"]["restored_exact"])
        self.assertTrue(artifact["cleanup"]["temp_root_removed"])
        # Both hdiutil attaches were detached.
        detaches = [c for c in mocks.calls if c[0] == "hdiutil" and c[1] == "detach"]
        self.assertEqual(len(detaches), 1)
        self.assertFalse((self.app_dir / "Fixture.app").exists())

    def test_rollback_digest_mismatch_ok_false(self) -> None:
        mocks = AuditMocks(self.mount_install, self.mount_upgrade, "Fixture.app")
        real_hash_bundle = AUDIT.hash_bundle
        calls = {"n": 0}

        def faulty_hash_bundle(bundle: Path):
            calls["n"] += 1
            if calls["n"] == 3:  # install=1, upgrade=2, rollback=3
                return "0" * 64, 0
            return real_hash_bundle(bundle)

        with mac_host(), patch.object(AUDIT, "_run", mocks), \
                patch.object(AUDIT, "hash_bundle", faulty_hash_bundle):
            code, _out, _err = run_main(AUDIT, self.base_args(execute=True))
        self.assertEqual(code, 1)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertFalse(artifact["ok"])
        self.assertFalse(artifact["rollback"]["matched_install_digest"])
        self.assertTrue(any(
            step["step"] == "verify_after_rollback" and step["status"] == "failed"
            for step in artifact["steps"]
        ))
        self.assertTrue(any("RollbackDigestMismatch" in e for e in artifact["errors"]))

    def test_uninstall_delete_failure_ok_false(self) -> None:
        mocks = AuditMocks(self.mount_install, self.mount_upgrade, "Fixture.app")
        real_rmtree = AUDIT.shutil.rmtree
        calls = {"n": 0}

        def faulty_rmtree(path, **kwargs):
            calls["n"] += 1
            # upgrade-remove=1, rollback-remove=2, uninstall-remove=3
            if calls["n"] == 3:
                raise OSError("permission denied")
            return real_rmtree(path, **kwargs)

        with mac_host(), patch.object(AUDIT, "_run", mocks), \
                patch.object(AUDIT.shutil, "rmtree", faulty_rmtree):
            code, _out, _err = run_main(AUDIT, self.base_args(execute=True))
        self.assertEqual(code, 1)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertFalse(artifact["ok"])
        self.assertTrue(any(
            step["step"] == "uninstall" and step["status"] == "failed"
            for step in artifact["steps"]
        ))
        # Cleanup still removed the bundle afterwards, restoring the empty
        # pre-state; the run fails because the uninstall step itself failed.
        self.assertTrue(artifact["cleanup"]["app_removed"])
        self.assertTrue(artifact["cleanup"]["restored_exact"])
        self.assertTrue(any("uninstall failed" in e for e in artifact["errors"]))
        self.assertFalse((self.app_dir / "Fixture.app").exists())

    def test_cleanup_detach_failure_ok_false(self) -> None:
        mocks = AuditMocks(self.mount_install, self.mount_upgrade, "Fixture.app")
        mocks.detach_fail_on = ["mnt-install"]
        with mac_host(), patch.object(AUDIT, "_run", mocks):
            code, _out, _err = run_main(AUDIT, self.base_args(execute=True))
        self.assertEqual(code, 1)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertFalse(artifact["ok"])
        self.assertFalse(artifact["cleanup"]["mounts_detached"])
        self.assertFalse(artifact["cleanup"]["restored_exact"])
        self.assertTrue(any("cleanup" in e for e in artifact["errors"]))

    def test_check_schema_valid_then_invalid(self) -> None:
        mocks = AuditMocks(self.mount_install, self.mount_upgrade, "Fixture.app")
        with mac_host(), patch.object(AUDIT, "_run", mocks):
            run_main(AUDIT, self.base_args(execute=True))
        code, _out, _err = run_main(
            AUDIT, ["--check-schema", "--artifact", str(self.artifact)]
        )
        self.assertEqual(code, 0)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        del data["cleanup"]
        self.artifact.write_text(json.dumps(data), encoding="utf-8")
        code, _out, err = run_main(
            AUDIT, ["--check-schema", "--artifact", str(self.artifact)]
        )
        self.assertEqual(code, 2)
        self.assertIn("cleanup", err)


class AutostartCredentialRoundtripTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.launch_agents = self.root / "LaunchAgents"
        self.launch_agents.mkdir()
        self.artifact = self.root / "roundtrip.json"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def base_args(self, execute: bool = True, platform: str = "auto") -> list[str]:
        args = [
            "--namespace", "roundtrip-ns",
            "--platform", platform,
            "--launch-agents-dir", str(self.launch_agents),
            "--candidate-commit", COMMIT,
            "--source-digest", DIGEST,
            "--artifact", str(self.artifact),
        ]
        if execute:
            args.append("--execute")
        return args

    def darwin_mock(self, retrievable: bool = True, delete_ok: bool = True):
        """Stateful darwin _run mock with optional fault injection."""
        state = {"loaded": False, "stored": False, "retrievable": retrievable}

        def fake_run(argv: list[str], **kwargs):
            fake_run.calls.append(argv)
            cmd = argv[0]
            if cmd == "launchctl":
                if argv[1] == "print":
                    return cp(0 if state["loaded"] else 1)
                if argv[1] == "bootstrap":
                    state["loaded"] = True
                    return cp(0)
                if argv[1] == "bootout":
                    state["loaded"] = False
                    return cp(0)
                return cp(1, stderr="unknown launchctl")
            if cmd == "security":
                if "find-generic-password" in argv and "-g" in argv:
                    return cp(0 if state["stored"] else 1)
                if "find-generic-password" in argv and "-w" in argv:
                    if state["stored"] and state["retrievable"]:
                        return cp(0, stdout=SECRET + "\n")
                    return cp(1)
                if "add-generic-password" in argv:
                    state["stored"] = True
                    return cp(0)
                if "delete-generic-password" in argv:
                    if not delete_ok:
                        return cp(1, stderr="delete failed")
                    state["stored"] = False
                    return cp(0)
                return cp(1, stderr="unknown security")
            return cp(0)

        fake_run.calls = []
        return fake_run

    def win32_mock(self):
        """Stateful win32 _run mock for reg + cmdkey."""
        state = {"run_present": False, "cred_present": False}

        def fake_run(argv: list[str], **kwargs):
            fake_run.calls.append(argv)
            cmd = argv[0]
            if cmd == "reg":
                if argv[1] == "query":
                    return cp(0 if state["run_present"] else 1)
                if argv[1] == "add":
                    state["run_present"] = True
                    return cp(0)
                if argv[1] == "delete":
                    state["run_present"] = False
                    return cp(0)
                return cp(1, stderr="unknown reg")
            if cmd == "cmdkey":
                if argv[1].startswith("/list:"):
                    return cp(0 if state["cred_present"] else 1)
                if argv[1].startswith("/add:"):
                    state["cred_present"] = True
                    return cp(0)
                if argv[1].startswith("/delete:"):
                    state["cred_present"] = False
                    return cp(0)
                return cp(1, stderr="unknown cmdkey")
            return cp(0)

        fake_run.calls = []
        return fake_run

    def test_default_mode_is_dry_run_with_no_side_effects(self) -> None:
        # Explicit --platform darwin keeps the expected plan text identical
        # on every CI host (auto would resolve to the host platform).
        with patch.object(ROUND, "_run") as mock_run:
            code, out, _err = run_main(
                ROUND, self.base_args(execute=False, platform="darwin")
            )
        self.assertEqual(code, 0)
        self.assertIn("dry-run: inputs valid", out)
        self.assertIn("LaunchAgent", out)
        self.assertIn("Keychain", out)
        mock_run.assert_not_called()
        self.assertFalse(self.artifact.exists())

    def test_dry_run_rejects_invalid_namespace(self) -> None:
        args = self.base_args(execute=False)
        args[args.index("roundtrip-ns")] = "../bad ns!"
        with patch.object(ROUND, "_run") as mock_run:
            code, _out, err = run_main(ROUND, args)
        self.assertEqual(code, 2)
        self.assertIn("namespace", err)
        mock_run.assert_not_called()

    def test_execute_without_binding_is_rejected(self) -> None:
        args = self.base_args(execute=True)
        args.remove("--candidate-commit")
        args.remove(COMMIT)
        with patch.object(ROUND, "_run") as mock_run:
            code, _out, err = run_main(ROUND, args)
        self.assertEqual(code, 2)
        self.assertIn("candidate-commit", err)
        mock_run.assert_not_called()

    def test_linux_platform_blocked(self) -> None:
        with patch.object(ROUND.sys, "platform", "linux"):
            code, _out, err = run_main(
                ROUND, self.base_args(execute=True, platform="auto")
            )
        self.assertEqual(code, 1)
        self.assertIn("platform-unsupported", err)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(artifact["status"], "blocked")
        self.assertFalse(artifact["ok"])
        self.assertIsNone(artifact["pre_state"]["entry_present"])
        self.assertIsNone(artifact["cleanup"]["entry_removed"])

    def test_linux_platform_dry_run_exits_2(self) -> None:
        with patch.object(ROUND.sys, "platform", "linux"):
            code, _out, err = run_main(
                ROUND, self.base_args(execute=False, platform="auto")
            )
        self.assertEqual(code, 2)
        self.assertIn("dry-run blocked", err)

    def test_macos_happy_path_verified(self) -> None:
        fake_run = self.darwin_mock()
        with darwin_host(), patch.object(ROUND, "_run", fake_run), \
                patch.object(ROUND, "_generate_secret", lambda: SECRET):
            code, out, _err = run_main(ROUND, self.base_args(execute=True))
        self.assertEqual(code, 0)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(artifact["status"], "verified")
        self.assertTrue(artifact["ok"])
        self.assertEqual(artifact["platform"], "darwin")
        self.assertEqual(artifact["candidate_commit"], COMMIT)
        self.assertEqual(artifact["autostart"]["provider"], "LaunchAgent")
        self.assertTrue(artifact["autostart"]["enable"]["ok"])
        self.assertTrue(artifact["autostart"]["disable"]["ok"])
        self.assertEqual(artifact["autostart"]["round_trips"], 2)
        self.assertEqual(artifact["credential"]["provider"], "Keychain")
        self.assertTrue(artifact["credential"]["set"]["verified"])
        self.assertTrue(artifact["credential"]["get"]["secret_matches"])
        self.assertTrue(artifact["credential"]["delete"]["verified"])
        self.assertEqual(artifact["credential"]["round_trips"], 2)
        self.assertIs(artifact["credential"]["plaintext_fallback"], False)
        self.assertEqual(artifact["pre_state"], {"entry_present": False})
        self.assertTrue(artifact["cleanup"]["entry_removed"])
        self.assertTrue(artifact["cleanup"]["restored_exact"])
        self.assertEqual(artifact["errors"], [])
        # Exact restore: no plist left behind, final store state empty.
        self.assertFalse((self.launch_agents / "roundtrip-ns.plist").exists())
        # No secret anywhere in the artifact or stdout.
        self.assertNotIn(SECRET, self.artifact.read_text(encoding="utf-8"))
        self.assertNotIn(SECRET, out)

    def test_launchctl_argv_contract(self) -> None:
        fake_run = self.darwin_mock()
        with darwin_host(), patch.object(ROUND, "_run", fake_run), \
                patch.object(ROUND, "_generate_secret", lambda: SECRET):
            run_main(ROUND, self.base_args(execute=True))
        bootstraps = [c for c in fake_run.calls if c[0] == "launchctl" and c[1] == "bootstrap"]
        bootouts = [c for c in fake_run.calls if c[0] == "launchctl" and c[1] == "bootout"]
        self.assertEqual(len(bootstraps), 2)
        self.assertEqual(len(bootouts), 2)
        for call in bootstraps:
            self.assertEqual(call[1:3], ["bootstrap", "gui/501"])
            self.assertEqual(Path(call[3]).name, "roundtrip-ns.plist")
        for call in bootouts:
            self.assertEqual(
                call[1], "bootout"
            )
            self.assertIn("com.dtoken.audit.roundtrip-ns", call[2])

    def test_launchagent_existing_entry_refused_before_any_command(self) -> None:
        plist = self.launch_agents / "roundtrip-ns.plist"
        plist.write_bytes(ROUND.build_launch_agent_plist("roundtrip-ns"))
        fake_run = self.darwin_mock()
        with darwin_host(), patch.object(ROUND, "_run", fake_run), \
                patch.object(ROUND, "_generate_secret", lambda: SECRET):
            code, _out, err = run_main(ROUND, self.base_args(execute=True))
        self.assertEqual(code, 2)
        self.assertIn("refusing existing same-name", err)
        self.assertEqual(fake_run.calls, [])
        self.assertTrue(plist.exists())
        self.assertFalse(self.artifact.exists())

    def test_keychain_existing_entry_refused_before_add(self) -> None:
        fake_run = self.darwin_mock()
        original = fake_run

        def with_existing(argv: list[str], **kwargs):
            if argv[0] == "security" and "find-generic-password" in argv and "-g" in argv:
                return cp(0)
            return original(argv, **kwargs)

        with darwin_host(), patch.object(ROUND, "_run", with_existing), \
                patch.object(ROUND, "_generate_secret", lambda: SECRET):
            code, _out, err = run_main(ROUND, self.base_args(execute=True))
        self.assertEqual(code, 2)
        self.assertIn("refusing existing same-name", err)
        adds = [c for c in original.calls if c[0] == "security" and "add-generic-password" in c]
        self.assertEqual(adds, [])
        self.assertFalse(self.artifact.exists())

    def test_post_create_verification_failure_ok_false_with_cleanup(self) -> None:
        # Entry is stored but never retrievable -> set verification fails.
        fake_run = self.darwin_mock(retrievable=False)
        with darwin_host(), patch.object(ROUND, "_run", fake_run), \
                patch.object(ROUND, "_generate_secret", lambda: SECRET):
            code, _out, _err = run_main(ROUND, self.base_args(execute=True))
        self.assertEqual(code, 1)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(artifact["status"], "failed")
        self.assertFalse(artifact["ok"])
        self.assertFalse(artifact["credential"]["set"]["verified"])
        self.assertFalse(artifact["credential"]["get"]["secret_matches"])
        # Cleanup still deleted the entry and restored the empty pre-state.
        self.assertTrue(artifact["cleanup"]["entry_removed"])
        deletes = [
            c for c in fake_run.calls
            if c[0] == "security" and "delete-generic-password" in c
        ]
        self.assertEqual(len(deletes), 2)
        self.assertTrue(any("set verification failed" in e for e in artifact["errors"]))

    def test_delete_failure_ok_false_with_restore_failure(self) -> None:
        fake_run = self.darwin_mock(delete_ok=False)
        with darwin_host(), patch.object(ROUND, "_run", fake_run), \
                patch.object(ROUND, "_generate_secret", lambda: SECRET):
            code, _out, _err = run_main(ROUND, self.base_args(execute=True))
        self.assertEqual(code, 1)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertFalse(artifact["ok"])
        self.assertFalse(artifact["credential"]["delete"]["ok"])
        self.assertFalse(artifact["cleanup"]["entry_removed"])
        self.assertFalse(artifact["cleanup"]["restored_exact"])
        self.assertTrue(any("delete failed" in e for e in artifact["errors"]))
        self.assertTrue(any("cleanup" in e for e in artifact["errors"]))

    def test_windows_happy_path_verified(self) -> None:
        fake_run = self.win32_mock()
        with patch.object(ROUND, "_run", fake_run), \
                patch.object(ROUND.sys, "platform", "win32"), \
                patch.object(ROUND, "_generate_secret", lambda: SECRET):
            code, _out, _err = run_main(
                ROUND, self.base_args(execute=True, platform="win32")
            )
        self.assertEqual(code, 0)
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(artifact["status"], "verified")
        self.assertTrue(artifact["ok"])
        self.assertEqual(artifact["autostart"]["provider"], "HKCU-Run")
        self.assertTrue(artifact["autostart"]["enable"]["verified"])
        self.assertTrue(artifact["autostart"]["disable"]["verified"])
        self.assertEqual(artifact["credential"]["provider"], "Credential-Manager")
        self.assertTrue(artifact["credential"]["get"]["secret_matches"])
        self.assertIs(artifact["credential"]["plaintext_fallback"], False)
        # Exact argv contracts.
        adds = [c for c in fake_run.calls if c[0] == "reg" and c[1] == "add"]
        self.assertEqual(len(adds), 2)
        self.assertEqual(adds[0][2], ROUND.HKCU_RUN)
        self.assertEqual(
            adds[0][3:],
            ["/v", "roundtrip-ns", "/t", "REG_SZ", "/d", "dtoken-audit-marker-roundtrip-ns", "/f"],
        )
        cmdkey_adds = [c for c in fake_run.calls if c[0] == "cmdkey" and c[1].startswith("/add:")]
        self.assertEqual(len(cmdkey_adds), 2)
        self.assertIn("dtoken-audit-roundtrip-ns", cmdkey_adds[0][1])
        self.assertNotIn(SECRET, self.artifact.read_text(encoding="utf-8"))

    def test_windows_blocked_on_macos_host_not_fabricated(self) -> None:
        with patch.object(ROUND.sys, "platform", "darwin"), \
                patch.object(ROUND, "_run") as mock_run:
            code, _out, err = run_main(
                ROUND, self.base_args(execute=True, platform="win32")
            )
        self.assertEqual(code, 1)
        self.assertIn("windows-host-unavailable", err)
        mock_run.assert_not_called()
        artifact = json.loads(self.artifact.read_text(encoding="utf-8"))
        self.assertEqual(artifact["status"], "blocked")
        self.assertFalse(artifact["ok"])
        self.assertTrue(any("windows-host-unavailable" in e for e in artifact["errors"]))
        # Blocked runs never probed the stores: unprobed facts are null.
        self.assertIsNone(artifact["pre_state"]["entry_present"])
        self.assertIsNone(artifact["cleanup"]["entry_removed"])
        self.assertIsNone(artifact["cleanup"]["restored_exact"])

    def test_check_schema_valid_then_invalid(self) -> None:
        fake_run = self.darwin_mock()
        with darwin_host(), patch.object(ROUND, "_run", fake_run), \
                patch.object(ROUND, "_generate_secret", lambda: SECRET):
            run_main(ROUND, self.base_args(execute=True))
        code, _out, _err = run_main(
            ROUND, ["--check-schema", "--artifact", str(self.artifact)]
        )
        self.assertEqual(code, 0)
        data = json.loads(self.artifact.read_text(encoding="utf-8"))
        data["credential"]["plaintext_fallback"] = True
        self.artifact.write_text(json.dumps(data), encoding="utf-8")
        code, _out, err = run_main(
            ROUND, ["--check-schema", "--artifact", str(self.artifact)]
        )
        self.assertEqual(code, 2)
        self.assertIn("plaintext_fallback", err)


if __name__ == "__main__":
    unittest.main()
