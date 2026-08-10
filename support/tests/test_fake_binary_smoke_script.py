"""
Contract tests for support/scripts/run_fake_binary_tests.sh.

Behavior-oriented regression tests for the fake-binary smoke runner. They
verify the script's structure and semantics WITHOUT launching cargo run or
touching the network:

* readiness is bounded and health/PID driven, never a fixed `sleep 2`.  The
  bounded loop may be written as `$(seq 1 N)`, `{1..N}`, or a C-style
  `for ((i=1; i<=N; i++))`, and the poll sleep must live inside that loop;
* every HTTP call carries an explicit curl max-time (`--max-time N`,
  `--max-time=N`, or `--max-time = N`);
* CARGO_HOME/RUSTUP_HOME are captured before HOME is overridden;
* fake binaries are staged into a temp home and only the copies are chmod'd
  (tracked support/fake-bin sources keep their committed modes);
* cleanup kills then waits the proxy (PID-guarded, tolerant; harmless
  statements between kill and wait are allowed), preserves the exit status,
  dumps only a bounded failure log (`sed -n '1,Np'` or `sed -n "1,Np"`),
  and removes TEST_HOME/LOG;
* a dead child must abort readiness -- `break` on the `kill -0` line itself
  (`|| break`) or inside the controlled block that follows it.

These tests assert contracts, not the script's implementation details, so a
reimplementation that keeps the same guarantees still passes. The snippet
cases pin both directions: equivalent bounded shell forms are accepted, and
unbounded loops, missing max-times, unbounded log dumps, zombie leaks, and
spinning on a dead PID are rejected.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "run_fake_binary_tests.sh"

# --- Semantic patterns (loose on quoting/whitespace, strict on meaning) ---

# Bounded readiness loop, in any of the equivalent finite forms:
#   for i in $(seq 1 N); for i in {1..N}; for ((i=1; i<=N; i++)).
BOUNDED_LOOP_RES = (
    re.compile(r"\$\s*\(\s*seq\s+1\s+(?P<bound>\d+)\s*\)"),
    re.compile(r"\{\s*(?P<start>\d+)\s*\.\.\s*(?P<end>\d+)\s*\}"),
    re.compile(
        r"for\s*\(\s*\(\s*[^;]*;\s*[^;]*?(?:<=|<)\s*(?P<bound>\d+)\s*;"
        r"\s*[^;]*\)\s*\)"
    ),
)
SLEEP_RE = re.compile(r"\bsleep\s+([0-9.]+)")
FIXED_SLEEP_RE = re.compile(r"\bsleep\s+[1-9][0-9]*\b")
# curl --max-time: space, `=`, or spaced-`=` separators are all equivalent.
MAX_TIME_RE = re.compile(r"--max-time\s*[=\s]\s*(\d+)")
# Bounded sed dump: '1,Np' or "1,Np".
SED_BOUND_RE = re.compile(r"sed\s+-n\s+[\"']1,(\d+)p[\"']")


# --- Contract helpers -------------------------------------------------------
# Each helper validates one contract against arbitrary script text, so the
# production script and the equivalent/defective snippets share one logic.


def bounded_loop(text: str) -> tuple[re.Match, int] | None:
    """Return (match, iteration count) of the first bounded readiness loop."""
    for pattern in BOUNDED_LOOP_RES:
        match = pattern.search(text)
        if match is None:
            continue
        groups = match.groupdict()
        if "end" in groups:
            iterations = int(groups["end"]) - int(groups["start"]) + 1
        else:
            iterations = int(groups["bound"])
        return match, iterations
    return None


def loop_body_region(text: str, loop_match: re.Match) -> str:
    """Text between the loop's `do` and its closing `done`."""
    do_idx = text.find("do", loop_match.end())
    if do_idx == -1:
        return ""
    done_idx = text.find("done", do_idx + 2)
    if done_idx == -1:
        return ""
    return text[do_idx + 2 : done_idx]


def loop_contract_violations(text: str) -> list[str]:
    """The readiness loop must be a finite for loop whose poll sleep lives
    inside the loop body, sub-second, with a bounded total wait."""
    loop = bounded_loop(text)
    if loop is None:
        return [
            "readiness loop must be a bounded for loop "
            "(seq / brace expansion / C-style arithmetic)"
        ]
    loop_match, iterations = loop
    if iterations < 1:
        return ["readiness loop must iterate at least once"]
    sleeps = [
        float(m.group(1))
        for m in SLEEP_RE.finditer(loop_body_region(text, loop_match))
    ]
    if not sleeps:
        return ["readiness loop must poll with a sleep inside the loop body"]
    if max(sleeps) > 1.0:
        return ["readiness poll delay must be sub-second"]
    if iterations * max(sleeps) > 300:
        return [f"total readiness wait {iterations}x{max(sleeps)}s is unbounded"]
    return []


def pid_dead_break_ok(lines: list[str], pid_idx: int) -> bool:
    """A dead child must abort the readiness loop: `break` on the same line
    as the `kill -0` check (`|| break`) or inside the controlled block that
    immediately follows it."""
    line = lines[pid_idx]
    if "break" in line:
        return True
    if not (line.lstrip().startswith("if") and 'kill -0 "$PID"' in line):
        return False  # PID-dead result is discarded: the loop would spin
    for candidate in lines[pid_idx + 1 : pid_idx + 6]:
        if "break" in candidate:
            return True
        if candidate.strip().startswith("fi"):
            return False  # controlled block closed without a break
    return False


def pid_dead_break_ok_in(text: str) -> bool:
    """pid_dead_break_ok applied to a standalone snippet."""
    lines = text.splitlines()
    pid_idx = next(
        (i for i, line in enumerate(lines) if 'kill -0 "$PID"' in line), None
    )
    if pid_idx is None:
        return False
    return pid_dead_break_ok(lines, pid_idx)


def curl_max_time_violations(lines: list[str]) -> list[str]:
    """Every non-comment curl call must carry an explicit --max-time >= 1."""
    violations = []
    for line in lines:
        if re.search(r"\bcurl\b", line) and not line.lstrip().startswith("#"):
            match = MAX_TIME_RE.search(line)
            if match is None:
                violations.append(f"curl without --max-time: {line!r}")
            elif int(match.group(1)) < 1:
                violations.append(f"curl --max-time must be >= 1s: {line!r}")
    return violations


def failure_log_violations(lines: list[str]) -> list[str]:
    """The failure log dump must be a bounded sed range over $LOG."""
    guard = next(
        (i for i, line in enumerate(lines) if "((status != 0))" in line), None
    )
    if guard is None:
        return ["failure path must branch on the captured exit status"]
    region = "\n".join(lines[guard : guard + 4])
    if "sed -n" not in region or '"$LOG"' not in region:
        return ["failure log dump must use a bounded sed over $LOG"]
    bound = SED_BOUND_RE.search(region)
    if bound is None:
        return [
            "failure log dump must use a bounded sed range "
            "('1,Np' or \"1,Np\")"
        ]
    if int(bound.group(1)) > 500:
        return ["failure log dump bound must be at most 500 lines"]
    return []


def cleanup_kill_wait_violations(body: str) -> list[str]:
    """cleanup must kill then wait the child, PID-guarded and tolerant.
    Harmless statements between kill and wait are allowed; their order is
    not negotiable (wait-before-kill or a missing wait leaks zombies)."""
    lines = body.splitlines()
    kill_idx = next(
        (i for i, line in enumerate(lines) if 'kill "$PID"' in line), None
    )
    if kill_idx is None:
        return ["cleanup() must kill the child PID"]
    wait_idx = next(
        (i for i, line in enumerate(lines) if 'wait "$PID"' in line), None
    )
    if wait_idx is None:
        return ["cleanup() must wait the child PID (no zombies)"]
    if wait_idx <= kill_idx:
        return ["cleanup must wait after kill"]
    if "|| true" not in lines[kill_idx]:
        return ["cleanup kill must be tolerant (|| true)"]
    if "|| true" not in lines[wait_idx]:
        return ["cleanup wait must be tolerant (|| true)"]
    if not any('[[ -n "$PID" ]]' in line for line in lines[:kill_idx]):
        return ["kill/wait must be guarded by a PID-set check"]
    return []


# --- Snippets: behaviorally equivalent forms the contract must accept -------

BRACE_LOOP_SNIPPET = """\
for attempt in {1..120}; do
  if ! kill -0 "$PID" >/dev/null 2>&1; then
    break
  fi
  if curl -fsS --max-time 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    READY=true
    break
  fi
  sleep 0.5
done
"""

CSTYLE_LOOP_SNIPPET = """\
for ((attempt = 1; attempt <= 120; attempt++)); do
  if ! kill -0 "$PID" >/dev/null 2>&1; then
    break
  fi
  if curl -fsS --max-time 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    READY=true
    break
  fi
  sleep 0.5
done
"""

MAX_TIME_EQUALS_SNIPPET = """\
curl -fsS --max-time=1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1
curl -fsS --max-time = 5 "http://127.0.0.1:${PORT}/rtk/status" | grep -q ok
"""

SED_DOUBLE_QUOTE_SNIPPET = """\
if ((status != 0)); then
  echo "fake binary smoke failed" >&2
  sed -n "1,200p" "$LOG" >&2 || true
fi
"""

CLEANUP_INTERVENING_SNIPPET = """\
cleanup() {
  status=$?
  trap - EXIT
  if [[ -n "$PID" ]]; then
    # reap the child so the exit path is deterministic
    kill "$PID" >/dev/null 2>&1 || true
    wait "$PID" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
"""

PID_BREAK_SAME_LINE_SNIPPET = """\
for attempt in $(seq 1 120); do
  kill -0 "$PID" >/dev/null 2>&1 || break
  if curl -fsS --max-time 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    READY=true
    break
  fi
  sleep 0.5
done
"""

PID_BREAK_BLOCK_SNIPPET = """\
for attempt in $(seq 1 120); do
  if ! kill -0 "$PID" >/dev/null 2>&1; then
    # child died before becoming ready
    break
  fi
  if curl -fsS --max-time 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    READY=true
    break
  fi
  sleep 0.5
done
"""

PID_BREAK_THEN_NEXT_LINE_SNIPPET = """\
for attempt in $(seq 1 120); do
  if ! kill -0 "$PID" >/dev/null 2>&1
  then
    break
  fi
  sleep 0.5
done
"""


# --- Snippets: real defects the contract must keep rejecting ----------------

WHILE_TRUE_SNIPPET = """\
while true; do
  if ! kill -0 "$PID" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
"""

CSTYLE_UNBOUNDED_SNIPPET = """\
for ((;;)); do
  if ! kill -0 "$PID" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
"""

SLEEP_OUTSIDE_LOOP_SNIPPET = """\
sleep 0.5
for attempt in $(seq 1 120); do
  if ! kill -0 "$PID" >/dev/null 2>&1; then
    break
  fi
done
"""

HUGE_SEQ_SNIPPET = """\
for attempt in $(seq 1 10000); do
  if ! kill -0 "$PID" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
"""

CURL_NO_MAX_TIME_SNIPPET = """\
curl -fsS "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1
"""

SED_UNBOUNDED_SNIPPET = """\
if ((status != 0)); then
  echo "fake binary smoke failed" >&2
  sed -n '1,$p' "$LOG" >&2 || true
fi
"""

LOG_CAT_SNIPPET = """\
if ((status != 0)); then
  echo "fake binary smoke failed" >&2
  cat "$LOG" >&2 || true
fi
"""

CLEANUP_NO_WAIT_SNIPPET = """\
cleanup() {
  status=$?
  if [[ -n "$PID" ]]; then
    kill "$PID" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
"""

CLEANUP_WAIT_FIRST_SNIPPET = """\
cleanup() {
  status=$?
  if [[ -n "$PID" ]]; then
    wait "$PID" >/dev/null 2>&1 || true
    kill "$PID" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
"""

CLEANUP_NO_GUARD_SNIPPET = """\
cleanup() {
  status=$?
  kill "$PID" >/dev/null 2>&1 || true
  wait "$PID" >/dev/null 2>&1 || true
  exit "$status"
}
"""

PID_DEAD_IGNORED_SNIPPET = """\
for attempt in $(seq 1 120); do
  kill -0 "$PID" >/dev/null 2>&1
  if curl -fsS --max-time 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    READY=true
    break
  fi
  sleep 0.5
done
"""


class FakeBinarySmokeContractTests(unittest.TestCase):
    """Structural/semantic contract tests for the smoke runner script."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.text = SCRIPT.read_text(encoding="utf-8")
        cls.lines = cls.text.splitlines()

    def line_index(self, needle: str) -> int:
        """First line index containing the needle; fail with context."""
        for i, line in enumerate(self.lines):
            if needle in line:
                return i
        self.fail(f"script has no line containing: {needle!r}")

    def test_script_exists(self) -> None:
        self.assertTrue(
            SCRIPT.is_file(), f"fake-binary smoke script missing: {SCRIPT}"
        )

    def test_shell_strictness(self) -> None:
        """Script must fail fast on unset vars, errors, and bad pipes."""
        self.assertIn("set -euo pipefail", self.text)

    # --- Readiness: bounded, PID+health driven, no fixed sleep ---

    def test_readiness_is_not_a_fixed_sleep(self) -> None:
        """A bare fixed `sleep 2` must never gate readiness again."""
        self.assertIsNone(
            FIXED_SLEEP_RE.search(self.text),
            "readiness must not depend on a fixed multi-second sleep",
        )
        # The only sleep allowed is the small in-loop poll delay.
        for match in SLEEP_RE.finditer(self.text):
            self.assertLessEqual(
                float(match.group(1)), 1.0,
                f"sleep values must be sub-second poll delays, got {match.group(0)}",
            )

    def test_readiness_loop_is_bounded(self) -> None:
        """Loop must iterate a finite count (seq / brace / C-style) with the
        poll sleep inside the loop body, so total wait time is bounded."""
        self.assertEqual(
            loop_contract_violations(self.text),
            [],
            "readiness loop must be bounded, with a sub-second in-loop sleep",
        )

    def test_readiness_checks_pid_and_health(self) -> None:
        """Ready only after: PID alive AND /health answers within max-time."""
        pid_line = self.line_index('kill -0 "$PID"')
        health_line = self.line_index("/health")
        ready_line = self.line_index("READY=true")
        self.assertLess(pid_line, health_line,
                        "PID liveness must be checked before the health probe")
        self.assertLess(health_line, ready_line,
                        "READY=true must only be set after a successful health probe")
        # A dead child must abort readiness instead of spinning.
        self.assertTrue(
            pid_dead_break_ok(self.lines, pid_line),
            "PID-dead must break the readiness loop (same-line `|| break` "
            "or a break in its controlled block)",
        )
        # Health probe itself is time-boxed.
        health_curl = next(
            (line for line in self.lines if "/health" in line and "curl" in line),
            "",
        )
        max_time = MAX_TIME_RE.search(health_curl)
        self.assertIsNotNone(max_time, "health probe must carry --max-time")
        self.assertLessEqual(int(max_time.group(1)), 5)

    def test_readiness_failure_is_an_explicit_failure(self) -> None:
        """Not becoming healthy must exit nonzero with a diagnostic."""
        self.assertIn("did not become healthy", self.text)
        self.assertIn('exit 1', self.text)

    # --- HTTP calls are always time-boxed ---

    def test_every_curl_has_max_time(self) -> None:
        """No curl invocation may hang the smoke run (space or `=` forms)."""
        self.assertEqual(
            curl_max_time_violations(self.lines),
            [],
            "every curl call must carry an explicit --max-time >= 1s",
        )

    # --- Toolchain capture before HOME override ---

    def test_toolchain_homes_captured_before_home_override(self) -> None:
        """CARGO_HOME/RUSTUP_HOME (with ${HOME} fallback) must be exported
        BEFORE `export HOME=$TEST_HOME`, so the real toolchain survives."""
        cargo_line = self.line_index('CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}"')
        rustup_line = self.line_index('RUSTUP_HOME="${RUSTUP_HOME:-${HOME}/.rustup}"')
        home_line = self.line_index('export HOME="$TEST_HOME"')
        self.assertLess(cargo_line, home_line,
                        "CARGO_HOME must be captured before HOME override")
        self.assertLess(rustup_line, home_line,
                        "RUSTUP_HOME must be captured before HOME override")
        # Explicit env vars are respected, not clobbered (CI / standalone cargo).
        self.assertIn("${CARGO_HOME:-", self.text)
        self.assertIn("${RUSTUP_HOME:-", self.text)

    # --- Fake-bin staging: copies chmod'd, tracked sources untouched ---

    def test_fake_bins_staged_into_temp_home(self) -> None:
        """Sources must be copied from the repo into a temp dir, not run
        in place (so a tracked chmod can never be introduced)."""
        self.assertIn('FAKE_BIN="$TEST_HOME/fake-bin"', self.text)
        for line in self.lines:
            if re.search(r"\bcp\b", line):
                self.assertIn("$ROOT/support/fake-bin/", line)
                self.assertIn("$FAKE_BIN", line)

    def test_only_staged_copies_are_chmodded(self) -> None:
        """chmod must target $FAKE_BIN copies only; never repo sources."""
        chmod_lines = [
            line
            for line in self.lines
            if re.search(r"\bchmod\s", line) and not line.lstrip().startswith("#")
        ]
        self.assertTrue(chmod_lines, "staged copies must be made executable")
        for line in chmod_lines:
            self.assertIn("$FAKE_BIN", line,
                          f"chmod must target the staged copy: {line!r}")
            self.assertNotIn(
                "support/fake-bin", line,
                f"tracked fake-bin sources must never be chmod'd: {line!r}",
            )

    # --- Fake-binary fidelity wiring ---

    def test_fake_binary_fidelity_env(self) -> None:
        """The isolated fake-binary profile must be fully wired up."""
        for contract in (
            'export PATH="$FAKE_BIN:$PATH"',
            'export HOME="$TEST_HOME"',
            'export DTOKEN_TEST_HOME="$TEST_HOME"',
            'export DTOKEN_FAKE_BIN="$FAKE_BIN"',
            "export DTOKEN_PROFILE=fake-binary",
            'export DTOKEN_FIXTURE_ROOT="$ROOT/support/fixtures"',
        ):
            self.assertIn(contract, self.text)
        # Fixtures must resolve relative to the isolated home.
        self.assertIn('ln -s "$ROOT/support/fixtures" "$TEST_HOME/fixtures"',
                      self.text)

    # --- Cleanup: kill+wait, fidelity, bounded log, full removal ---

    def test_cleanup_kills_and_waits_proxy(self) -> None:
        """Cleanup must kill the child and reap it, tolerantly, PID-guarded,
        and only when a PID was actually started."""
        cleanup_start = self.line_index("cleanup()")
        body = "\n".join(self.lines[cleanup_start:])
        self.assertEqual(
            cleanup_kill_wait_violations(body),
            [],
            "cleanup must kill then wait the PID, tolerantly and PID-guarded",
        )
        self.assertIn("trap cleanup EXIT", self.text)
        self.assertIn("trap - EXIT", self.text)

    def test_cleanup_preserves_exit_status(self) -> None:
        """The script must re-exit with the original status, so failure
        propagates to CI after cleanup runs."""
        self.assertIn('exit "$status"', self.text)

    def test_cleanup_failure_log_is_bounded(self) -> None:
        """On failure the proxy log dump must be capped, not streamed."""
        self.assertEqual(
            failure_log_violations(self.lines),
            [],
            "failure log dump must use a bounded sed range",
        )

    def test_cleanup_registered_before_staging(self) -> None:
        """cleanup() and `trap cleanup EXIT` must be defined before the
        first fallible staging command (mkdir/cp/chmod/ln), so a `set -e`
        failure during staging cannot leak TEST_HOME."""
        staging_commands = (
            r"\bmkdir\b",
            r"\bcp\b",
            r"\bchmod\b",
            r"\bln\b",
        )
        staging_idx = min(
            i
            for i, line in enumerate(self.lines)
            for pattern in staging_commands
            if re.search(pattern, line) and not line.lstrip().startswith("#")
        )
        cleanup_idx = self.line_index("cleanup()")
        trap_idx = self.line_index("trap cleanup EXIT")
        self.assertLess(cleanup_idx, staging_idx,
                        "cleanup() must be defined before staging commands")
        self.assertLess(trap_idx, staging_idx,
                        "trap cleanup EXIT must be registered before staging commands")

    def test_cleanup_removes_test_home_and_log(self) -> None:
        """Both the temp home and the log must be removed on exit."""
        self.assertIn('rm -rf "$TEST_HOME" "$LOG"', self.text)

    # --- Equivalent positive forms: contract, not shell text ---

    def test_equivalent_loop_forms_accepted(self) -> None:
        """Brace expansion and C-style finite loops are equivalent to
        `$(seq 1 N)` for the bounded readiness loop."""
        for label, snippet in (
            ("brace expansion", BRACE_LOOP_SNIPPET),
            ("C-style arithmetic", CSTYLE_LOOP_SNIPPET),
        ):
            with self.subTest(loop_form=label):
                self.assertEqual(loop_contract_violations(snippet), [])

    def test_max_time_equals_form_accepted(self) -> None:
        """`--max-time=N` (and spaced-`=` forms) are equivalent to
        `--max-time N`."""
        self.assertEqual(
            curl_max_time_violations(MAX_TIME_EQUALS_SNIPPET.splitlines()),
            [],
        )

    def test_sed_double_quoted_accepted(self) -> None:
        """`sed -n "1,Np"` is equivalent to the single-quoted bounded dump."""
        self.assertEqual(
            failure_log_violations(SED_DOUBLE_QUOTE_SNIPPET.splitlines()),
            [],
        )

    def test_cleanup_intervening_statements_accepted(self) -> None:
        """Reasonable statements between kill and wait are allowed; only
        their order within cleanup() is part of the contract."""
        self.assertEqual(
            cleanup_kill_wait_violations(CLEANUP_INTERVENING_SNIPPET),
            [],
        )

    def test_pid_dead_break_same_line_accepted(self) -> None:
        """`kill -0 "$PID" ... || break` is a valid dead-child abort."""
        self.assertTrue(pid_dead_break_ok_in(PID_BREAK_SAME_LINE_SNIPPET))

    def test_pid_dead_break_controlled_block_accepted(self) -> None:
        """A break inside the controlled block following the `kill -0` check
        (comment lines allowed) is a valid dead-child abort."""
        self.assertTrue(pid_dead_break_ok_in(PID_BREAK_BLOCK_SNIPPET))

    def test_pid_dead_break_then_on_next_line_accepted(self) -> None:
        """`then` on its own line inside the controlled block is still
        equivalent."""
        self.assertTrue(pid_dead_break_ok_in(PID_BREAK_THEN_NEXT_LINE_SNIPPET))

    # --- Missing-behavior negatives: real defects stay rejected ---

    def test_unbounded_loop_forms_rejected(self) -> None:
        """while-true and `for ((;;))` loops never satisfy the bounded-loop
        contract, no matter how many features they share with it."""
        for label, snippet in (
            ("while true", WHILE_TRUE_SNIPPET),
            ("for ((;;))", CSTYLE_UNBOUNDED_SNIPPET),
        ):
            with self.subTest(loop_form=label):
                self.assertTrue(
                    loop_contract_violations(snippet),
                    f"{label} must be rejected as unbounded",
                )

    def test_loop_sleep_outside_loop_rejected(self) -> None:
        """A sleep outside the loop body does not bound the readiness loop."""
        self.assertTrue(
            loop_contract_violations(SLEEP_OUTSIDE_LOOP_SNIPPET),
            "poll sleep must be inside the readiness loop body",
        )

    def test_loop_total_wait_rejected_when_unbounded(self) -> None:
        """Finite loops with excessive iterations still make the total wait
        unbounded."""
        self.assertTrue(
            loop_contract_violations(HUGE_SEQ_SNIPPET),
            "total readiness wait must stay bounded (<= 300s)",
        )

    def test_curl_without_max_time_rejected(self) -> None:
        """A curl call with no max-time must fail the contract."""
        violations = curl_max_time_violations(CURL_NO_MAX_TIME_SNIPPET.splitlines())
        self.assertTrue(violations)
        self.assertIn("curl without --max-time", violations[0])

    def test_sed_unbounded_range_rejected(self) -> None:
        """`sed -n '1,$p'` streams the whole log: still a defect."""
        self.assertTrue(
            failure_log_violations(SED_UNBOUNDED_SNIPPET.splitlines()),
            "unbounded sed range must be rejected",
        )

    def test_log_cat_dump_rejected(self) -> None:
        """Dumping the log with cat is unbounded by construction."""
        self.assertTrue(
            failure_log_violations(LOG_CAT_SNIPPET.splitlines()),
            "unbounded cat dump must be rejected",
        )

    def test_cleanup_missing_wait_rejected(self) -> None:
        """Kill without wait leaves a zombie: rejected."""
        self.assertTrue(cleanup_kill_wait_violations(CLEANUP_NO_WAIT_SNIPPET))

    def test_cleanup_wait_before_kill_rejected(self) -> None:
        """Wait-before-kill breaks reap ordering: rejected."""
        self.assertTrue(
            cleanup_kill_wait_violations(CLEANUP_WAIT_FIRST_SNIPPET),
            "wait must come after kill",
        )

    def test_cleanup_kill_not_pid_guarded_rejected(self) -> None:
        """Kill/wait without a PID-set guard touches the empty PID: rejected."""
        self.assertTrue(
            cleanup_kill_wait_violations(CLEANUP_NO_GUARD_SNIPPET),
            "kill/wait must be guarded by a PID-set check",
        )

    def test_pid_dead_ignored_rejected(self) -> None:
        """Ignoring the `kill -0` result lets the loop spin on a dead child:
        rejected."""
        self.assertFalse(pid_dead_break_ok_in(PID_DEAD_IGNORED_SNIPPET))


if __name__ == "__main__":
    unittest.main()
