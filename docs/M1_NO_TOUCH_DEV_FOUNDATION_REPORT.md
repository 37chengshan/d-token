# M1 No-touch Dev Foundation Report

Status: implemented for the current skeleton.

## Scope

M1 establishes development-safe behavior: fixture-first execution, fake-binary support, no default calls to real RTK or Headroom, and no default writes to real agent/tool configuration.

## Implemented

1. Fixture-backed RTK and Headroom adapter paths.
2. Runtime mode modeling for fixture, fake-binary, shadow, external, managed, and built-in fallback modes.
3. Development Doctor checks for profile, fake binary guardrails, temporary HOME expectations, and plan-only config patching.
4. `CompressionPolicy::Max` support so the Rust workspace compiles.
5. Fixture smoke scripts for Bash and PowerShell.
6. Windows fake-binary shims (`rtk.cmd`, `headroom.cmd`) alongside Unix fake binaries.
7. PRD updated to make macOS, Windows native, Windows WSL, all-agent coverage, and OpenAI/Anthropic protocol injection explicit requirements.

## Verification

1. `python support/scripts/check_structure.py` passed.
2. `cargo check --workspace` passed.
3. `support/scripts/run_fixture_tests.ps1` passed structure and cargo-check gates.
4. `cargo test --workspace` could not run on this machine because the active Rust host is `x86_64-pc-windows-gnu` and `dlltool.exe` is missing.

## Multidimensional Review

Product: aligns with the PRD requirement that development defaults must not touch real RTK, Headroom, agent configs, hooks, or services.

Engineering: adapters now have explicit mode boundaries instead of silently probing real binaries by default.

Security/privacy: default fixture mode avoids reading real tee files, CCR cache, shell profiles, or user configs.

Testing: compile and structure gates are green; runtime tests are scriptable but skipped locally until the Windows GNU linker toolchain is complete.

Risk: fake-binary and fixture coverage is still shallow compared with a full no-touch smoke matrix.

Platform review: M1 now includes Unix shell fake binaries and Windows `.cmd` fake shims, so Mac/Linux-style and Windows-style command lookup can be validated without calling real tools.
