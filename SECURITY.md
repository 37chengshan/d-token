# Security Policy

## Supported versions

| Version | Security support |
|---|---|
| `0.0.1` | Best-effort security fixes while the 0.0.1 line is active |
| Earlier development snapshots | Unsupported |

Candidate builds are not a promise of production suitability. `0.0.1` is an unsigned candidate technical preview for local evaluation, with preview targets macOS ARM64 and Windows x64. It is not a signed, notarized, Authenticode-signed, or publicly distributable installer release. An unsigned or ad-hoc macOS build may show a system security warning; verify the source and any supplied hash, and do not bypass operating-system security controls for untrusted packages. A candidate is publicly distributable only when its tagged commit passes the applicable signed platform release workflow.

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability, exposed credential, unsafe configuration write, signature bypass, path traversal, local privilege escalation, or sensitive-data disclosure.

Use GitHub's private vulnerability reporting for the repository. Include:

- affected version, commit, platform and architecture;
- the smallest reproducible sequence;
- expected and observed behavior;
- security impact and affected data or boundary;
- relevant sanitized logs, screenshots or crash information;
- whether the issue requires a malicious local file, network peer, Agent configuration, imported component, or user confirmation.

Do not include real API keys, OAuth credentials, cookies, Authorization headers, private prompts, source code from projects you do not own, or personal data. Replace secrets with clearly marked placeholders.

Maintainers should acknowledge a complete report within seven calendar days when possible. Remediation and disclosure timing depend on severity, reproducibility and release availability. No bounty or monetary reward is promised.

## Security boundaries

The normative technical boundaries are documented in [`docs/SECURITY_BOUNDARIES.md`](docs/SECURITY_BOUNDARIES.md). Important release-facing properties include:

- local services bind to loopback;
- secrets are stored through the operating-system credential store and ordinary configuration contains only opaque references;
- logs, analytics and exports must not contain complete prompts, responses, source files or credentials;
- configuration writes require preview, redacted diff, durable backup, explicit confirmation, verification and rollback evidence;
- imported Code Graph components are pinned and verified by platform, architecture, version and SHA-256;
- release packages fail closed when signing, notarization, platform verification or mandatory artifacts are missing;
- unsupported repair, routing or transform paths remain unavailable rather than simulating success.

## Release integrity

The technical preview must not be represented as satisfying the official-release list below. Missing signing credentials, notarization, platform verification, or mandatory artifacts cause the official workflow to fail closed rather than turning an incomplete preview into a release.

Official release assets must be attached to a GitHub Release created from a matching version tag and must include:

- signed platform installer;
- SHA-256 manifest;
- release notes and build information;
- CycloneDX SBOM;
- dependency license inventory;
- basic security scan report.

For `0.0.1`, macOS publication additionally requires Developer ID signing, notarization, stapling, Gatekeeper assessment, temporary installation, startup and uninstall validation. Windows must not be described as publishable until the real Windows workflow verifies Authenticode signatures before and after installation, launches the installed application, audits bundle contents and completes silent uninstall.

## Out of scope

The following are not vulnerabilities by themselves:

- expected Beta limitations already documented in the README or changelog;
- unavailable features that fail closed without exposing data or changing configuration;
- local denial of service requiring the same user's unrestricted filesystem access, unless it crosses a documented trust boundary or causes silent data loss;
- findings that depend only on unmodified local reference directories such as `headroom-0.31.0/`, `.codex/`, `.ai-bridge/` or `test/artifacts/`, because those directories are not part of the release distribution.

## Safe research

Test only systems and data you own or are authorized to assess. Avoid persistence, destructive changes, service disruption, privacy violations and accessing other users' information. Stop testing and report immediately if you encounter credentials, private source code or personal data outside your authorized scope.
