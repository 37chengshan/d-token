#!/usr/bin/env python3
"""Fail-closed agent config acceptance runner (candidate probe contract).

Runs the formal agent transport candidate probe against *copies* of agent
config files staged inside a caller-provided temp root, so the original
config files are never modified. Each probe is executed as a subprocess:

    cargo run -p dtoken-config-patcher \
        --example agent_transport_candidate_probe -- \
        AGENT_ID COPY BACKUP ENDPOINT MODEL

Only the seven formal single-file agent adapters are accepted; any other
agent id is explicitly blocked before anything runs. auth.json and filenames
containing credential/key/token/cookie markers, plus symlinks and
non-regular files, are rejected before anything is staged (fail-closed).
The probe's public JSON report is verified (configure.verified,
clear.verified, restore.state == restored_exact, restore.sha256 matching the
source fingerprint, mode preserved, and the network/process/raw/credential/
model public booleans all false); the artifact never emits the probe target
or model values, never contains file contents, and never contains absolute
paths.

Evidence contract
-----------------
The JSON artifact is a single top-level object with exactly eight keys:
schema_version, status, mode, candidate_commit, source_digest, artifact_sha256,
agents, restore_order. schema_version is the artifact schema version; status
is one of verified|failed|blocked; mode is always executed (--dry-run and
--check-schema never write an artifact); candidate_commit is the repository
HEAD commit and source_digest is the unified N0 definition — SHA-256 of the
canonical JSON of {tracked path: blob sha256} over every tracked file at the
candidate commit (git ls-tree + git cat-file --batch, paths sorted by UTF-8;
gitignored/untracked content never enters), computed by the shared
canonical_source_digest helper so every evidence artifact binds the exact
same candidate tree (N9 bound mode, resolved fail-closed before execution);
artifact_sha256 is SHA-256 of canonical JSON without the artifact_sha256 field;
the aggregator recomputes it exactly using generate_release_gate_status.
agents is an array of per-agent records containing exactly agent_id, adapter,
source_fingerprint, source_mode, status, error_class, steps; restore_order is
the list of agent ids exact-restored during a rollback, in the actual
reverse order in which they were restored. The artifact never contains file
contents, probe target paths, model values, or absolute paths. Source files
are only ever stat()ed and read once through a no-follow fd; copies are staged
in the temp root as fresh files; their bytes are never printed. After every
probe every source is
re-verified for both sha256 and permission mode; any change fails the run
closed and every staged copy is exact-restored in reverse order. Agents run
sequentially; on any failure, every previously succeeded agent's staged copy
is exact-restored in reverse order.

An unsupported agent id in a real execution writes a blocked artifact
(status=blocked) instead of a bare validation exit; rejected auth/secret
names, symlinks and non-regular files fail closed before any artifact is
written. --dry-run validates inputs without executing anything and
--check-schema validates an existing artifact against this schema; neither
mode emits evidence of a run that never happened.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import signal
import string
import subprocess
import sys
import tempfile
from pathlib import Path
from stat import S_IMODE, S_ISDIR, S_ISLNK, S_ISREG
from typing import Any, Optional

DEFAULT_ARTIFACT_NAME = "agent-config-acceptance.json"
DEFAULT_ENDPOINT = "http://127.0.0.1:18001"
DEFAULT_MODEL = "unknown"
PROBE_TIMEOUT = 300

STATUS_OK = "ok"
STATUS_FAILED = "failed"

# O_NOFOLLOW is unavailable on Windows; there the no-symlink guarantee is
# enforced with lstat checks before/after the open plus an inode comparison
# (see read_no_follow). O_BINARY keeps os.open byte-exact on Windows.
_O_NOFOLLOW = getattr(os, "O_NOFOLLOW", 0)
_O_BINARY = getattr(os, "O_BINARY", 0)

# Top-level artifact contract: one object with exactly these keys. status is
# one of verified|failed|blocked and mode is always executed; --dry-run and
# --check-schema never write an artifact. candidate_commit and source_digest
# bind the evidence to a concrete candidate revision (N9 bound mode):
# candidate_commit is the repository HEAD commit and source_digest is the
# {tracked path: blob sha256} over every tracked file at the candidate commit
# (canonical_source_digest.compute_source_manifest), a content-bearing anchor
# that is never empty; both bindings are resolved before any run.
# artifact_sha256 is SHA-256 of the canonical JSON of the artifact WITHOUT the
# artifact_sha256 field (aggregator recompute rule).
SCHEMA_VERSION = "1.1"
STATUS_VERIFIED = "verified"
STATUS_BLOCKED = "blocked"
MODE_EXECUTED = "executed"
ARTIFACT_KEYS = frozenset(
    {
        "schema_version",
        "status",
        "mode",
        "candidate_commit",
        "source_digest",
        "artifact_sha256",
        "agents",
        "restore_order",
    }
)
ARTIFACT_STATUSES = frozenset({STATUS_VERIFIED, STATUS_FAILED, STATUS_BLOCKED})

# The seven formal single-file agent transports backed by
# dtoken-config-patcher examples/agent_transport_candidate_probe.rs.
ADAPTERS = frozenset(
    {
        "claude-code",
        "codex",
        "cline",
        "gemini-cli",
        "roo-code",
        "continue",
        "aider",
    }
)

ALLOWED_AGENT_KEYS = frozenset(
    {
        "agent_id",
        "adapter",
        "source_fingerprint",
        "source_mode",
        "status",
        "error_class",
        "steps",
    }
)
ALLOWED_STEP_KEYS = frozenset({"step", "status"})
STEP_STATUSES = frozenset({STATUS_OK, STATUS_FAILED})

REJECTED_EXACT_NAMES = frozenset({"auth.json"})
REJECTED_NAME_MARKERS = ("credential", "key", "token", "cookie")

# Public booleans the candidate probe report must keep false for a
# read-only shadow acceptance run.
PUBLIC_BOOLEANS = (
    "network_request_made",
    "agent_process_started",
    "raw_config_returned",
    "credential_value_read",
    "model_returned",
)

# Minimal environment whitelist for the probe subprocess: the probe is
# candidate code under acceptance, so it never inherits the caller's full
# environment. Only allowlisted names survive; any name carrying a
# credential-like marker is dropped even if allowlisted.
ENV_ALLOWLIST = frozenset(
    {
        "PATH",
        "HOME",
        "USERPROFILE",
        "TMP",
        "TEMP",
        "TMPDIR",
        "RUSTUP_HOME",
        "CARGO_HOME",
        "RUSTUP_TOOLCHAIN",
        "RUSTFLAGS",
        "CARGO_BUILD_JOBS",
        "CARGO_TARGET_DIR",
        "SCCACHE_DIR",
        "SHELL",
        "COMSPEC",
        "PATHEXT",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TERM",
        "USER",
        "LOGNAME",
        "PWD",
    }
)
ENV_REJECT_MARKERS = (
    "CREDENTIAL",
    "TOKEN",
    "KEY",
    "COOKIE",
    "AUTH",
    "SECRET",
    "PASSWORD",
    "PROVIDER",
)

REPO_ROOT = Path(__file__).resolve().parents[2]
CARGO_DIR = REPO_ROOT / "code"

# Shared N0 candidate binding helper (VERIFICATION.md §1): the single source
# of truth for candidate_commit and canonical tracked-file source_digest.
_SCRIPTS_DIR = Path(__file__).resolve().parent
if str(_SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS_DIR))
from canonical_source_digest import (  # noqa: E402
    SourceDigestError,
    resolve_candidate_binding,
)


class AcceptanceError(Exception):
    error_class = "AcceptanceError"


class UnsupportedAgent(AcceptanceError):
    error_class = "UnsupportedAgent"


class AuthFileRejected(AcceptanceError):
    error_class = "AuthFileRejected"


class ConfigMissing(AcceptanceError):
    error_class = "ConfigMissing"


class ConfigNotFile(AcceptanceError):
    error_class = "ConfigNotFile"


class SymlinkRejected(AcceptanceError):
    error_class = "SymlinkRejected"


class ProbeFailed(AcceptanceError):
    error_class = "ProbeFailed"


class ProbeReportInvalid(AcceptanceError):
    error_class = "ProbeReportInvalid"


class GitBindingUnresolved(AcceptanceError):
    error_class = "GitBindingUnresolved"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact_sha256(artifact: dict[str, Any]) -> str:
    """artifact_sha256 rule (generate_release_gate_status): SHA-256 of the
    canonical JSON of the artifact WITHOUT the artifact_sha256 field."""
    body = {key: value for key, value in artifact.items() if key != "artifact_sha256"}
    canonical = json.dumps(
        body, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return sha256_bytes(canonical)


def hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_unchanged(
    source: Path,
    initial_fingerprint: str,
    initial_mode: str,
    initial_dev: int,
    initial_ino: int,
) -> Optional[str]:
    """Return the error class if a source changed since staging.

    The guard is fail-closed on all three dimensions: the source must still
    be a regular file, its sha256 must match the fingerprint taken before
    the run, its permission mode must match the mode taken before the run,
    and its device/inode must match the pre-run snapshot (a same-bytes
    replacement is still a modification). Returns None when the source is
    unchanged.
    """
    try:
        data, mode, dev, ino = read_no_follow(source)
    except OSError:
        return "SourceModified"
    if sha256_bytes(data) != initial_fingerprint:
        return "SourceModified"
    if oct(mode) != initial_mode:
        return "SourceModified"
    if (dev, ino) != (initial_dev, initial_ino):
        return "SourceModified"
    return None


def is_rejected_name(name: str) -> bool:
    lower = name.lower()
    if lower in REJECTED_EXACT_NAMES:
        return True
    return any(marker in lower for marker in REJECTED_NAME_MARKERS)


def read_no_follow(path: Path) -> tuple[bytes, int, int, int]:
    """Read a regular file through one fd without following a symlink.

    Returns (bytes, mode, st_dev, st_ino) all taken from the same fstat, so
    the fingerprint, mode and inode can never span file generations. On
    platforms without O_NOFOLLOW (Windows) the entry is lstat-checked
    before and after the open and the opened inode must match the lstat'd
    entry, so a substituted symlink is still rejected.
    """
    st = path.lstat()
    fd = os.open(path, os.O_RDONLY | _O_NOFOLLOW | _O_BINARY)
    try:
        opened = os.fstat(fd)
        chunks: list[bytes] = []
        while True:
            chunk = os.read(fd, 65536)
            if not chunk:
                break
            chunks.append(chunk)
    finally:
        os.close(fd)
    if (
        not S_ISREG(st.st_mode)
        or not S_ISREG(opened.st_mode)
        or (st.st_dev, st.st_ino) != (opened.st_dev, opened.st_ino)
    ):
        raise OSError(f"entry is not a stable regular file: {path.name}")
    return b"".join(chunks), S_IMODE(opened.st_mode), opened.st_dev, opened.st_ino


def write_fresh(path: Path, data: bytes, mode: int) -> None:
    """Write a fresh regular file at path, never through an existing entry.

    Any existing entry — including a regular file that may be a hard link
    to an inode outside the temp root — is unlinked first, and the new file
    is created with O_EXCL, so the write can never land in a pre-existing
    inode. A directory at the slot fails closed.
    """
    try:
        existing = path.lstat()
    except FileNotFoundError:
        pass
    else:
        if S_ISDIR(existing.st_mode):
            raise OSError(f"path is a directory: {path.name}")
        try:
            path.unlink()
        except PermissionError:  # Windows: read-only entries cannot be unlinked
            os.chmod(path, 0o600)
            path.unlink()
    fd = os.open(
        path,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | _O_NOFOLLOW | _O_BINARY,
        mode,
    )
    try:
        view = memoryview(data)
        while view:
            written = os.write(fd, view)
            if written == 0:
                raise RuntimeError(f"write stalled: {path.name}")
            view = view[written:]
    finally:
        os.close(fd)
    os.chmod(path, mode)
    if not S_ISREG(path.lstat().st_mode):
        raise OSError(f"path is not a regular file after write: {path.name}")


def restore_exact(copy_path: Path, pristine: bytes, mode: int) -> None:
    """Restore a staged copy slot to its pristine bytes and permission bits.

    Fail-closed against symlink *and hard-link* substitution: the slot is
    unlinked unconditionally and re-created fresh with O_EXCL, so the
    restore never writes through a link or pre-existing inode. The result
    is re-read via read_no_follow and verified byte- and mode-exact.
    """
    write_fresh(copy_path, pristine, mode)
    restored, restored_mode, _, _ = read_no_follow(copy_path)
    if restored != pristine or restored_mode != mode:
        raise RuntimeError(
            f"byte-exact restore verification failed: {copy_path.name}"
        )


def probe_argv(
    agent_id: str,
    copy_path: Path,
    backup_root: Path,
    endpoint: str,
    model: str,
) -> list[str]:
    """The exact candidate probe subprocess argv contract."""
    return [
        "cargo",
        "run",
        "-p",
        "dtoken-config-patcher",
        "--example",
        "agent_transport_candidate_probe",
        "--",
        agent_id,
        str(copy_path),
        str(backup_root),
        endpoint,
        model,
    ]


def probe_env() -> dict[str, str]:
    """Build the minimal probe subprocess environment.

    Only allowlisted variables survive; any variable whose name carries a
    credential-like marker (credential/token/key/cookie/auth/secret/
    password/provider) is dropped even if allowlisted. The probe never
    inherits the caller's full environment.
    """
    return {
        name: value
        for name, value in (
            (name, os.environ.get(name)) for name in ENV_ALLOWLIST
        )
        if value is not None
        and not any(marker in name.upper() for marker in ENV_REJECT_MARKERS)
    }


def _kill_process_group(proc: subprocess.Popen) -> None:
    """Kill the probe's whole process group after a timeout.

    ``cargo run`` spawns the probe binary as a child; killing only the
    direct child would orphan the running probe. The probe runs in its own
    session (start_new_session), so on POSIX its process group id equals
    its pid. The group is only killed while the direct child is still
    running (proc.poll() is None): once the pid has been reaped it may be
    recycled by an unrelated process, and killpg on a recycled pid could
    signal an innocent process group. On Windows process groups are
    unavailable; the direct child is killed best-effort.
    """
    if hasattr(os, "killpg"):
        if proc.poll() is not None:
            return
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
        except OSError:
            pass
    else:
        proc.kill()


def _communicate_probe(
    argv: list[str],
    cwd: str,
    env: dict[str, str],
    timeout: int,
) -> subprocess.CompletedProcess:
    """Run the probe subprocess; on timeout kill the whole process group.

    The probe is started as a new session leader so a timeout can kill the
    entire group (cargo plus the probe binary) instead of orphaning the
    probe. Output is decoded with errors="replace" so a non-UTF-8 probe
    never escapes as a decoding crash. The Popen context manager guarantees
    proc.stdout/proc.stderr are closed on every path — including the
    timeout path, where communicate() abandons the pipes open — so the
    runner never leaks file descriptors (no ResourceWarning).
    """
    with subprocess.Popen(
        argv,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        errors="replace",
        env=env,
        start_new_session=True,
    ) as proc:
        try:
            stdout, stderr = proc.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            _kill_process_group(proc)
            proc.wait()
            raise
    return subprocess.CompletedProcess(argv, proc.returncode, stdout, stderr)


def run_probe(
    agent_id: str,
    copy_path: Path,
    backup_root: Path,
    endpoint: str,
    model: str,
) -> dict[str, Any]:
    """Run the candidate probe and return its parsed public JSON report.

    Raises ProbeFailed if the process cannot run, times out, exits
    non-zero, or emits no parseable JSON object. The report's
    target/model fields are never propagated to the caller's artifact.
    """
    argv = probe_argv(agent_id, copy_path, backup_root, endpoint, model)
    try:
        result = _communicate_probe(
            argv, cwd=str(CARGO_DIR), env=probe_env(), timeout=PROBE_TIMEOUT
        )
    except subprocess.TimeoutExpired as exc:
        raise ProbeFailed(f"probe timed out after {PROBE_TIMEOUT}s") from exc
    except OSError as exc:
        raise ProbeFailed(f"probe process failed to run: {exc}") from exc
    if result.returncode != 0:
        raise ProbeFailed(f"probe exited with code {result.returncode}")
    candidate: Optional[str] = None
    for line in reversed(result.stdout.splitlines()):
        stripped = line.strip()
        if stripped.startswith("{"):
            candidate = stripped
            break
    if candidate is None:
        raise ProbeFailed("probe produced no JSON report")
    try:
        report = json.loads(candidate)
    except ValueError as exc:
        raise ProbeFailed("probe JSON report unparseable") from exc
    if not isinstance(report, dict):
        raise ProbeFailed("probe JSON report is not an object")
    return report


def verify_probe_report(
    report: dict[str, Any],
    fingerprint: str,
    copy_path: Path,
    pristine: bytes,
    mode: int,
    source_dev: int,
    source_ino: int,
) -> None:
    """Verify the public booleans/hashes of a probe report; fail-closed.

    The staged copy must be byte- and mode-exact after the probe and must
    never alias the source inode (a hard-link substitution to the real
    config would make a later rollback write through to the source). The
    copy's own inode is intentionally NOT compared: the candidate probe
    restores atomically (temp file + rename), which legitimately replaces
    the inode on POSIX. Only an alias to the source is rejected.
    """
    configure = report.get("configure")
    clear = report.get("clear")
    restore = report.get("restore")
    if not isinstance(configure, dict) or configure.get("verified") is not True:
        raise ProbeReportInvalid("configure.verified is not true")
    if not isinstance(clear, dict) or clear.get("verified") is not True:
        raise ProbeReportInvalid("clear.verified is not true")
    if not isinstance(restore, dict) or restore.get("state") != "restored_exact":
        raise ProbeReportInvalid("restore.state is not restored_exact")
    if restore.get("sha256") != fingerprint:
        raise ProbeReportInvalid("restore.sha256 does not match source fingerprint")
    if restore.get("mode_preserved") is not True:
        raise ProbeReportInvalid("restore.mode_preserved is not true")
    for flag in PUBLIC_BOOLEANS:
        if report.get(flag) is not False:
            raise ProbeReportInvalid(f"{flag} must be false in a candidate probe")
    try:
        current, current_mode, current_dev, current_ino = read_no_follow(copy_path)
    except OSError:
        raise ProbeReportInvalid(
            "staged copy missing or not a regular file after probe"
        ) from None
    if current != pristine:
        raise ProbeReportInvalid("staged copy not byte-exact after probe")
    if current_mode != mode:
        raise ProbeReportInvalid("staged copy mode not preserved after probe")
    if (current_dev, current_ino) == (source_dev, source_ino):
        raise ProbeReportInvalid(
            "staged copy aliases the source inode after probe"
        )


def run_agent(
    record: dict[str, Any],
    agent_id: str,
    source: Path,
    index: int,
    temp_root: Path,
    endpoint: str,
    model: str,
    snapshot: tuple[Path, bytes, str, str, int, int, int],
) -> tuple[Path, bytes, int]:
    """Stage one agent config and run the candidate probe against the copy.

    ``snapshot`` holds the source's pre-run (path, bytes, fingerprint,
    mode string, mode, st_dev, st_ino), captured once before any probe ran;
    the staged copy is written from those bytes as a fresh file (never
    through an existing entry), so it can never reflect a later source
    mutation or follow a link. The probe is verified against the same
    pre-run bytes and mode; the post-probe copy must not alias the source
    inode (atomic-replace restore legitimately replaces the copy inode).
    If the staging slot already resolves to the source or aliases its
    inode, the agent fails closed before anything is unlinked or written.

    Raises AcceptanceError on any failure; the failing staged copy is
    best-effort restored to its pre-run bytes before the error propagates.
    On success returns the (copy_path, original_bytes, source_mode) handle
    needed for a later reverse-order rollback.
    """
    _, original, fingerprint, mode_str, mode, dev, ino = snapshot
    if agent_id not in ADAPTERS:
        raise UnsupportedAgent(f"unsupported agent id: {agent_id}")
    if is_rejected_name(source.name):
        raise AuthFileRejected(f"rejected auth/secret config file: {source.name}")
    try:
        st = source.lstat()
    except OSError as exc:
        raise ConfigMissing(f"cannot stat config: {source.name}") from exc
    if S_ISLNK(st.st_mode):
        raise SymlinkRejected(f"symlink config rejected: {source.name}")
    if not S_ISREG(st.st_mode):
        raise ConfigNotFile(f"not a regular file: {source.name}")
    record["source_fingerprint"] = fingerprint
    record["source_mode"] = mode_str

    copy_path = temp_root / f"agent_{index}_{source.name}"
    # Fail closed BEFORE any unlink/write at the staging slot: if the slot
    # resolves to the source itself, or already holds an entry aliasing the
    # source inode (e.g. a pre-seeded hard link to the source), refuse to
    # touch it. The source is never unlinked, replaced or written through.
    if copy_path.resolve() == source.resolve():
        raise ConfigNotFile(f"staged copy path resolves to the source: {source.name}")
    try:
        slot_st = copy_path.lstat()
    except FileNotFoundError:
        slot_st = None
    if slot_st is not None and (slot_st.st_dev, slot_st.st_ino) == (dev, ino):
        raise ConfigNotFile(
            f"staged copy slot already aliases the source inode: {source.name}"
        )
    try:
        write_fresh(copy_path, original, mode)
    except OSError as exc:
        raise ConfigMissing(f"cannot stage config copy: {source.name}") from exc
    copy_st = copy_path.lstat()
    if (copy_st.st_dev, copy_st.st_ino) == (dev, ino):
        raise ConfigNotFile(f"staged copy aliases the source inode: {source.name}")
    record["steps"].append({"step": "copy", "status": STATUS_OK})
    backup_root = temp_root / "backups" / f"agent_{index}"
    try:
        report = run_probe(agent_id, copy_path, backup_root, endpoint, model)
        record["steps"].append({"step": "probe", "status": STATUS_OK})
        verify_probe_report(
            report,
            record["source_fingerprint"],
            copy_path,
            original,
            mode,
            dev,
            ino,
        )
        record["steps"].append({"step": "verify", "status": STATUS_OK})
    except AcceptanceError:
        try:
            restore_exact(copy_path, original, mode)
        except Exception:
            pass
        raise
    return copy_path, original, mode


def write_artifact_atomic(artifact_path: Path, artifact: dict[str, Any]) -> None:
    """Write the artifact atomically and privately.

    The payload is written to a same-directory temp file with an
    unpredictable name (tempfile.mkstemp, O_EXCL, mode 0600), fsynced,
    chmod'ed 0600 and os.replace()d over the final path, so a crash can
    never leave a partial artifact, readers never observe a half-written
    file, and a pre-seeded entry at a guessable name can neither be
    written through nor wedge the write. On POSIX the parent directory is
    fsynced after the replace so the rename itself is durable; the
    directory fsync is skipped on Windows (a directory cannot be opened
    there) and is best-effort on filesystems that reject it. The temp
    file is removed on any failure.
    """
    artifact_path.parent.mkdir(parents=True, exist_ok=True)
    tmp_name: Optional[str] = None
    try:
        fd, tmp_name = tempfile.mkstemp(
            prefix=f".{artifact_path.name}.",
            suffix=".tmp",
            dir=str(artifact_path.parent),
        )
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(json.dumps(artifact, indent=2) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(tmp_name, 0o600)
        os.replace(tmp_name, artifact_path)
        if os.name == "posix":
            try:
                dir_fd = os.open(artifact_path.parent, os.O_RDONLY)
            except OSError:
                pass
            else:
                try:
                    os.fsync(dir_fd)
                except OSError:
                    pass
                finally:
                    os.close(dir_fd)
    except BaseException:
        if tmp_name is not None:
            try:
                os.unlink(tmp_name)
            except OSError:
                pass
        raise


def resolve_git_bindings(repo_root: Path) -> tuple[str, str]:
    """Resolve HEAD and the exact N0 canonical tracked-file source digest.

    Any Git or manifest failure aborts before executed evidence is written.
    """
    try:
        return resolve_candidate_binding(repo_root, "HEAD")
    except SourceDigestError as exc:
        raise GitBindingUnresolved(f"cannot resolve candidate binding: {exc}") from exc


def run_acceptance(
    agent_specs: list[tuple[str, Path]],
    temp_root: Path,
    artifact_path: Path,
    endpoint: str,
    model: str,
    candidate_commit: str,
    source_digest: str,
) -> tuple[str, list[dict[str, Any]], list[str]]:
    """Run all agents sequentially; after every probe re-verify every
    original source for both sha256 and permission mode. On any failure or
    source change, exact-restore every staged copy in reverse order back to
    the pre-run snapshot bytes, so a source mutated before its agent was
    staged is still rolled back to the content that existed before the run.
    candidate_commit/source_digest (resolved by the caller) are embedded in
    the written artifact to bind the evidence to the candidate revision.

    Returns (status, records, restore_order) where status is verified|failed
    and restore_order is the list of agent ids that were exact-restored, in
    the actual reverse order in which the restore happened. The artifact is
    written as a top-level object with exactly ARTIFACT_KEYS; a record that
    cannot carry a source fingerprint (a source deleted or replaced by a
    symlink after the snapshot) fails closed with no artifact written,
    mirroring the unsupported-adapter blocked path.
    """
    records: list[dict[str, Any]] = []
    done: list[tuple[tuple[Path, bytes, int], dict[str, Any]]] = []
    # Pre-run snapshot: source bytes, mode and inode are captured from one
    # no-follow fd and hashed from the very bytes kept for rollback, so the
    # restore baseline and the hash guard can never drift apart (no
    # read/hash race). Any OSError here (source vanished or replaced by a
    # symlink after validation) fails closed before anything runs or is
    # written.
    source_snapshots: list[
        tuple[Path, bytes, str, str, int, int, int]
    ] = []
    for _, source in agent_specs:
        try:
            st = source.lstat()
        except OSError as exc:
            raise ConfigMissing(
                f"cannot stat config before run: {source.name}"
            ) from exc
        if S_ISLNK(st.st_mode):
            raise SymlinkRejected(f"symlink config rejected: {source.name}")
        if not S_ISREG(st.st_mode):
            raise ConfigNotFile(f"not a regular file: {source.name}")
        try:
            original, mode, dev, ino = read_no_follow(source)
        except OSError as exc:
            raise ConfigMissing(
                f"cannot read config before run: {source.name}"
            ) from exc
        source_snapshots.append(
            (
                source,
                original,
                sha256_bytes(original),
                oct(mode),
                mode,
                dev,
                ino,
            )
        )
    failed_index: Optional[int] = None

    for index, (agent_id, source) in enumerate(agent_specs):
        record: dict[str, Any] = {
            "agent_id": agent_id,
            "adapter": agent_id,
            "source_fingerprint": None,
            "source_mode": None,
            "status": STATUS_OK,
            "error_class": None,
            "steps": [],
        }
        records.append(record)
        failed = False
        try:
            handle = run_agent(
                record,
                agent_id,
                source,
                index,
                temp_root,
                endpoint,
                model,
                source_snapshots[index],
            )
        except AcceptanceError as exc:
            record["status"] = STATUS_FAILED
            record["error_class"] = exc.error_class
            failed = True
        except Exception:
            record["status"] = STATUS_FAILED
            record["error_class"] = "UnexpectedFailure"
            failed = True
        else:
            done.append((handle, record))

        # Fail-closed after every probe: every original source whose agent
        # has been staged so far must still match its pre-run sha256 AND
        # permission mode. Any change stops the run and every staged copy is
        # exact-restored in reverse order.
        source_modified = False
        for snap_index in range(len(records)):
            snap_source, _, initial_hash, initial_mode, _, snap_dev, snap_ino = (
                source_snapshots[snap_index]
            )
            reason = source_unchanged(
                snap_source, initial_hash, initial_mode, snap_dev, snap_ino
            )
            if reason is not None:
                records[snap_index]["status"] = STATUS_FAILED
                records[snap_index]["error_class"] = reason
                source_modified = True

        if failed or source_modified:
            failed_index = index
            break

    # Final fail-closed re-verification covering the window between the last
    # probe and the artifact write; a late change still triggers the
    # reverse-order restore of every staged copy.
    for index in range(len(records)):
        source, _, initial_hash, initial_mode, _, snap_dev, snap_ino = (
            source_snapshots[index]
        )
        reason = source_unchanged(
            source, initial_hash, initial_mode, snap_dev, snap_ino
        )
        if reason is not None:
            records[index]["status"] = STATUS_FAILED
            records[index]["error_class"] = reason
            if failed_index is None:
                failed_index = len(records)

    restore_order: list[str] = []
    if failed_index is not None:
        for (copy_path, pristine, mode), record in reversed(done):
            try:
                restore_exact(copy_path, pristine, mode)
            except Exception:
                record["status"] = STATUS_FAILED
                record["error_class"] = "RollbackFailed"
            else:
                restore_order.append(record["agent_id"])
                record["steps"].append(
                    {"step": "rollback_restore", "status": STATUS_OK}
                )

    status = (
        STATUS_FAILED
        if any(record["status"] == STATUS_FAILED for record in records)
        else STATUS_VERIFIED
    )
    # Fail-closed before the artifact write: a record whose source was
    # deleted or replaced by a symlink/non-regular file between the
    # pre-run snapshot and its staging has no fingerprint to certify.
    # Writing it would violate the tool's own schema, so the run exits
    # without an artifact, mirroring the unsupported-adapter blocked path.
    for record in records:
        if (
            record["source_fingerprint"] is None
            or record["source_mode"] is None
        ):
            raise ConfigMissing(
                f"cannot fingerprint source for {record['agent_id']}, "
                "no artifact written"
            )
    artifact = {
        "schema_version": SCHEMA_VERSION,
        "status": status,
        "mode": MODE_EXECUTED,
        "candidate_commit": candidate_commit,
        "source_digest": source_digest,
        "agents": records,
        "restore_order": restore_order,
    }
    artifact["artifact_sha256"] = artifact_sha256(artifact)
    write_artifact_atomic(artifact_path, artifact)
    return artifact["status"], records, restore_order


def iter_strings(value: Any, prefix: str = ""):
    if isinstance(value, str):
        yield prefix, value
    elif isinstance(value, dict):
        for key, item in value.items():
            child = f"{prefix}.{key}" if prefix else str(key)
            yield from iter_strings(item, child)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            child = f"{prefix}[{index}]" if prefix else f"[{index}]"
            yield from iter_strings(item, child)


def is_absolute_like(value: str) -> bool:
    if os.path.isabs(value):
        return True
    if len(value) >= 2 and value[0].isalpha() and value[1] == ":":
        # Windows drive paths: C:\\... and drive-relative C:foo
        return True
    if value.startswith("\\\\") or value.startswith("//"):
        # UNC paths, e.g. \\host\share or //host/share
        return True
    return False


def check_artifact_schema(artifact_path: Path) -> list[str]:
    """Validate an artifact against the evidence contract; fail-closed."""
    try:
        data = json.loads(Path(artifact_path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        return [f"artifact unreadable or invalid JSON: {exc}"]
    violations: list[str] = []
    if not isinstance(data, dict):
        return [
            "artifact must be a top-level JSON object with exactly "
            "schema_version/status/mode/candidate_commit/source_digest/"
            "artifact_sha256/agents/restore_order"
        ]

    extra = sorted(set(data) - ARTIFACT_KEYS)
    if extra:
        violations.append(f"artifact disallowed keys: {extra}")
    missing = sorted(ARTIFACT_KEYS - set(data))
    if missing:
        violations.append(f"artifact missing keys: {missing}")
    if not isinstance(data.get("schema_version"), str) or not data["schema_version"]:
        violations.append("schema_version must be a non-empty string")
    if data.get("status") not in ARTIFACT_STATUSES:
        violations.append("status must be verified|failed|blocked")
    if data.get("mode") != MODE_EXECUTED:
        violations.append("mode must be executed")
    candidate_commit = data.get("candidate_commit")
    if (
        not isinstance(candidate_commit, str)
        or len(candidate_commit) not in (40, 64)
        or any(char not in string.hexdigits for char in candidate_commit)
    ):
        violations.append(
            "candidate_commit must be a git commit hash (40 or 64 hex chars)"
        )
    source_digest = data.get("source_digest")
    if (
        not isinstance(source_digest, str)
        or len(source_digest) != 64
        or any(char not in string.hexdigits for char in source_digest)
    ):
        violations.append(
            "source_digest must be a sha256 hex digest (64 hex chars)"
        )
    declared_sha = data.get("artifact_sha256")
    recomputed = artifact_sha256(data)
    if not isinstance(declared_sha, str) or declared_sha != recomputed:
        violations.append(
            "artifact_sha256 mismatch (must equal SHA-256 of the canonical "
            "JSON without the artifact_sha256 field)"
        )

    agents = data.get("agents")
    if not isinstance(agents, list) or not agents:
        violations.append("agents must be a non-empty array of agent records")
    else:
        for index, record in enumerate(agents):
            if not isinstance(record, dict):
                violations.append(f"agents[{index}] must be an object")
                continue
            extra_record = sorted(set(record) - ALLOWED_AGENT_KEYS)
            if extra_record:
                violations.append(f"agents[{index}] disallowed keys: {extra_record}")
            missing_record = sorted(ALLOWED_AGENT_KEYS - set(record))
            if missing_record:
                violations.append(f"agents[{index}] missing keys: {missing_record}")
            if not isinstance(record.get("agent_id"), str) or not record["agent_id"]:
                violations.append(f"agents[{index}] agent_id must be a non-empty string")
            if not isinstance(record.get("adapter"), str) or not record["adapter"]:
                violations.append(f"agents[{index}] adapter must be a non-empty string")
            if not isinstance(record.get("source_fingerprint"), str) or not record[
                "source_fingerprint"
            ]:
                violations.append(
                    f"agents[{index}] source_fingerprint must be a non-empty string"
                )
            if not isinstance(record.get("source_mode"), str) or not record["source_mode"]:
                violations.append(
                    f"agents[{index}] source_mode must be a non-empty string"
                )
            if record.get("status") not in STEP_STATUSES:
                violations.append(f"agents[{index}] status must be ok|failed")
            if record.get("error_class") is not None and not isinstance(
                record.get("error_class"), str
            ):
                violations.append(f"agents[{index}] error_class must be a string or null")
            steps = record.get("steps")
            if not isinstance(steps, list):
                violations.append(f"agents[{index}] steps must be a list")
                continue
            for step_index, step in enumerate(steps):
                if not isinstance(step, dict):
                    violations.append(
                        f"agents[{index}].steps[{step_index}] must be an object"
                    )
                    continue
                extra_step = sorted(set(step) - ALLOWED_STEP_KEYS)
                if extra_step:
                    violations.append(
                        f"agents[{index}].steps[{step_index}] disallowed keys: {extra_step}"
                    )
                if not isinstance(step.get("step"), str) or not step["step"]:
                    violations.append(
                        f"agents[{index}].steps[{step_index}] step must be a non-empty string"
                    )
                if step.get("status") not in STEP_STATUSES:
                    violations.append(
                        f"agents[{index}].steps[{step_index}] status must be ok|failed"
                    )

    restore_order = data.get("restore_order")
    if not isinstance(restore_order, list):
        violations.append("restore_order must be an array of agent ids")
    else:
        # agents may be missing or not a list; the violation above already
        # reported that, so never iterate the raw value here.
        agent_ids = (
            {
                record.get("agent_id")
                for record in agents
                if isinstance(record, dict)
            }
            if isinstance(agents, list)
            else set()
        )
        seen: set[str] = set()
        for order_index, agent_id in enumerate(restore_order):
            if not isinstance(agent_id, str) or agent_id not in agent_ids:
                violations.append(
                    f"restore_order[{order_index}] must be an agent id present in agents"
                )
            elif agent_id in seen:
                violations.append(
                    f"restore_order[{order_index}] duplicate agent id: {agent_id}"
                )
            else:
                seen.add(agent_id)

    for location, value in iter_strings(data):
        if is_absolute_like(value):
            violations.append(f"absolute path leaked at {location}: {value!r}")
    return violations


def parse_agent_specs(raw_specs: list[str]) -> tuple[list[tuple[str, Path]], list[str]]:
    """Parse --agent AGENT_ID=PATH specs; malformed specs become problems.

    A duplicate agent id is a problem too: a repeated id would stage two
    copies for one agent and, on rollback, write a restore_order that the
    tool's own schema checker rejects as duplicated.
    """
    parsed: list[tuple[str, Path]] = []
    problems: list[str] = []
    seen_ids: set[str] = set()
    for spec in raw_specs:
        if "=" not in spec:
            problems.append(f"invalid --agent spec (expected AGENT_ID=PATH): {spec}")
            continue
        agent_id, _, raw_path = spec.partition("=")
        agent_id = agent_id.strip()
        if not agent_id:
            problems.append(f"empty agent id in spec: {spec}")
            continue
        if agent_id in seen_ids:
            problems.append(f"duplicate agent id: {agent_id}")
            continue
        seen_ids.add(agent_id)
        parsed.append((agent_id, Path(raw_path)))
    return parsed, problems


def validate_inputs(
    agent_specs: list[tuple[str, Path]], temp_root: Path
) -> list[str]:
    """Fail-closed pre-run validation shared by --dry-run and the real run.

    Reports environment and unsafe-source problems only; unsupported agent
    ids are handled separately (blocked artifact in a real execution, plain
    validation failure in --dry-run).
    """
    problems: list[str] = []
    if not agent_specs:
        problems.append("no agent specs provided (--agent AGENT_ID=PATH required)")
    if not temp_root.exists():
        problems.append(f"temp root does not exist: {temp_root.name}")
    elif not temp_root.is_dir():
        problems.append(f"temp root is not a directory: {temp_root.name}")
    elif not os.access(temp_root, os.W_OK):
        problems.append(f"temp root is not writable: {temp_root.name}")
    for index, (agent_id, path) in enumerate(agent_specs):
        if path.is_symlink():
            problems.append(f"agent[{index}] symlink config rejected: {path.name}")
        elif not path.exists():
            problems.append(f"agent[{index}] missing: {path.name}")
        elif not path.is_file():
            problems.append(f"agent[{index}] not a regular file: {path.name}")
        elif is_rejected_name(path.name):
            problems.append(
                f"agent[{index}] rejected auth/secret file: {path.name}"
            )
    return problems


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="run_agent_config_acceptance.py",
        description=(
            "Run the formal agent transport candidate probe on copies of "
            "agent configs staged in a temp root. Source config files are "
            "never modified; the artifact is a top-level JSON object with "
            "schema_version/status/mode/candidate_commit/source_digest/"
            "agents/restore_order, bound to the candidate revision (git "
            "HEAD + tree digest) and never contains content, target, "
            "model, or absolute paths."
        ),
    )
    parser.add_argument(
        "--agent",
        action="append",
        dest="agents",
        metavar="AGENT_ID=PATH",
        help=(
            "agent id and source config path; agent id must be one of "
            f"{', '.join(sorted(ADAPTERS))} (repeatable)"
        ),
    )
    parser.add_argument(
        "--temp-root",
        metavar="DIR",
        help="existing writable directory that stages the copies",
    )
    parser.add_argument(
        "--endpoint",
        default=DEFAULT_ENDPOINT,
        metavar="URL",
        help=f"probe endpoint (default: {DEFAULT_ENDPOINT})",
    )
    parser.add_argument(
        "--model",
        default=DEFAULT_MODEL,
        metavar="MODEL",
        help=f"probe model id (default: {DEFAULT_MODEL})",
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--dry-run",
        action="store_true",
        help="validate inputs and print the plan without running anything (fail-closed)",
    )
    mode.add_argument(
        "--check-schema",
        action="store_true",
        help="validate the JSON artifact schema and redaction contract, then exit (fail-closed)",
    )
    parser.add_argument(
        "--artifact",
        metavar="PATH",
        help=f"artifact output path (default: <temp-root>/{DEFAULT_ARTIFACT_NAME})",
    )
    return parser


def main(argv: Optional[list[str]] = None) -> int:
    args = build_parser().parse_args(argv)

    if args.check_schema:
        if args.artifact:
            artifact_path = Path(args.artifact)
        elif args.temp_root:
            artifact_path = Path(args.temp_root) / DEFAULT_ARTIFACT_NAME
        else:
            artifact_path = Path(DEFAULT_ARTIFACT_NAME)
        violations = check_artifact_schema(artifact_path)
        if violations:
            for violation in violations:
                print(f"schema violation: {violation}", file=sys.stderr)
            return 2
        print(f"artifact schema valid: {artifact_path.name}")
        return 0

    if not args.agents:
        print(
            "validation failed: no agent specs provided "
            "(--agent AGENT_ID=PATH required)",
            file=sys.stderr,
        )
        return 2
    if not args.temp_root:
        print("validation failed: --temp-root DIR is required", file=sys.stderr)
        return 2
    temp_root = Path(args.temp_root)
    artifact_path = (
        Path(args.artifact) if args.artifact else temp_root / DEFAULT_ARTIFACT_NAME
    )

    agent_specs, spec_problems = parse_agent_specs(args.agents)
    unsupported = sorted(
        {agent_id for agent_id, _ in agent_specs if agent_id not in ADAPTERS}
    )
    problems = spec_problems + validate_inputs(agent_specs, temp_root)
    if args.dry_run:
        if problems or unsupported:
            for problem in problems:
                print(f"dry-run failed: {problem}", file=sys.stderr)
            for agent_id in unsupported:
                print(
                    f"dry-run failed: unsupported agent id: {agent_id} "
                    f"(known: {', '.join(sorted(ADAPTERS))})",
                    file=sys.stderr,
                )
            return 2
        print(
            f"dry-run: inputs valid; nothing executed (would run "
            f"{len(agent_specs)} candidate probe(s) inside '{temp_root.name}')"
        )
        return 0
    if problems:
        for problem in problems:
            print(f"validation failed: {problem}", file=sys.stderr)
        return 2
    # N9 bound mode: every executed artifact must be bound to a concrete
    # candidate revision. Resolution happens once, before either the
    # blocked or the real run path, and fails closed with no artifact if
    # the repository state cannot be established.
    try:
        candidate_commit, source_digest = resolve_git_bindings(REPO_ROOT)
    except AcceptanceError as exc:
        print(f"run failed: {exc}", file=sys.stderr)
        return 2
    if unsupported:
        # A real execution with an unsupported adapter writes a blocked
        # structured artifact instead of exiting without evidence. The
        # sources were already validated (regular, non-symlink); if a
        # fingerprint cannot be taken now (e.g. the file vanished or was
        # replaced by a symlink in a race), fail closed WITHOUT an artifact,
        # because a blocked artifact with null fingerprints would violate
        # the tool's own schema. Only genuinely unsupported ids are labeled
        # UnsupportedAgent; supported ids in the same run were blocked, not
        # unsupported, and are labeled RunBlocked.
        records: list[dict[str, Any]] = []
        for agent_id, source in agent_specs:
            try:
                data, mode, _, _ = read_no_follow(source)
            except OSError:
                print(
                    f"blocked: cannot fingerprint source, no artifact written: "
                    f"{source.name}",
                    file=sys.stderr,
                )
                return 2
            records.append(
                {
                    "agent_id": agent_id,
                    "adapter": agent_id,
                    "source_fingerprint": sha256_bytes(data),
                    "source_mode": oct(mode),
                    "status": STATUS_FAILED,
                    "error_class": (
                        "UnsupportedAgent"
                        if agent_id in unsupported
                        else "RunBlocked"
                    ),
                    "steps": [],
                }
            )
        artifact = {
            "schema_version": SCHEMA_VERSION,
            "status": STATUS_BLOCKED,
            "mode": MODE_EXECUTED,
            "candidate_commit": candidate_commit,
            "source_digest": source_digest,
            "agents": records,
            "restore_order": [],
        }
        artifact["artifact_sha256"] = artifact_sha256(artifact)
        try:
            write_artifact_atomic(artifact_path, artifact)
        except OSError as exc:
            print(
                f"blocked: cannot write artifact: {exc}",
                file=sys.stderr,
            )
            return 1
        for index, record in enumerate(records):
            print(
                f"agent[{index}] {record['agent_id']} {record['status']} "
                f"error_class={record['error_class']}"
            )
        print(f"blocked: unsupported agent(s) {', '.join(unsupported)}")
        print(f"artifact: {artifact_path.name}")
        print(
            f"blocked: unsupported agent id(s): {', '.join(unsupported)}",
            file=sys.stderr,
        )
        return 2

    try:
        status, records, restore_order = run_acceptance(
            agent_specs,
            temp_root,
            artifact_path,
            args.endpoint,
            args.model,
            candidate_commit,
            source_digest,
        )
    except AcceptanceError as exc:
        print(f"run failed: {exc}", file=sys.stderr)
        return 2
    except OSError as exc:
        # artifact write failed (e.g. artifact path is a directory or the
        # temp file cannot be created): clean exit, no traceback.
        print(f"run failed: cannot write artifact: {exc}", file=sys.stderr)
        return 1
    for index, record in enumerate(records):
        print(
            f"agent[{index}] {record['agent_id']} {record['status']} "
            f"error_class={record['error_class']}"
        )
    print(
        f"artifact: {artifact_path.name} "
        f"(status={status}, mode={MODE_EXECUTED}, "
        f"restore_order={restore_order})"
    )
    return 0 if status == STATUS_VERIFIED else 1


if __name__ == "__main__":
    sys.exit(main())
