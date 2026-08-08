# d-token Third-Party Notices

Last updated: 2026-08-08
Applies to: d-token `0.2.0-beta.1` source and desktop distribution candidates

This file is the distribution-facing third-party notice for d-token. The project itself is licensed under the Apache License 2.0; see [`LICENSE`](LICENSE). Release installers must include both `LICENSE` and this file under their legal resources.

The generated dependency license inventory shipped with each release candidate is supplementary. If it conflicts with a pinned-source notice below, the pinned-source notice and the applicable upstream license control.

## Distribution policy

The d-token desktop installer contains the d-token application, its Rust runtime/launchers, WebView support files required by the target platform, and legal notices. It does **not** bundle versioned local Headroom reference directories (`headroom-*/`), `.codex/`, `.ai-bridge/`, `.planning/`, `test/artifacts/`, or other workspace-only material.

The installer does **not** bundle the large `codebase-memory-mcp` upstream executable. Code Graph users explicitly import the pinned upstream component after installation. d-token verifies the selected platform, architecture, version and SHA-256 before use. The installer may include the upstream license and notice text for transparency and for display during the import flow; including those legal files does not mean the executable is redistributed in the installer.

## AiMaMi-derived UI adaptations

- Upstream: `borawong/AiMaMi`
- Pinned commit: `297c7af56f10fb371b77bc9b6b65aa320afcbe7e`
- Copyright: Copyright 2025-2026 borawong
- License: Apache License 2.0
- Preserved license copy: [`docs/legal/AIMAMI_APACHE_2_0_LICENSE.txt`](docs/legal/AIMAMI_APACHE_2_0_LICENSE.txt)

The following d-token files contain adapted structure or interaction concepts and have been substantially modified for d-token:

- `code/apps/desktop/src/app/components/shell/PageStage.tsx`
- `code/apps/desktop/src/app/components/shell/AnimatedSegmentedControl.tsx`
- `code/apps/desktop/src/app/components/shell/Sidebar.tsx`
- `code/apps/desktop/src/styles/shell.css`

The d-token distribution does not include AiMaMi names, logos, application icons, screenshots, people, QR codes, branded copy, trademarks, sound assets, or Type4Me audio.

## Spleen 5x8 bitmap font data

- Upstream: `fcambus/spleen`, `spleen-5x8.bdf`, version 2.2.0
- Copyright: Copyright (c) 2018-2026, Frederic Cambus
- License: BSD 2-Clause License
- Distributed subset: printable ASCII glyph bitmap data in `code/crates/dtoken-compress/src/optical/spleen5x8.rs`

Copyright (c) 2018-2026, Frederic Cambus
All rights reserved.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Codebase Memory MCP import integration

- Upstream: `DeusData/codebase-memory-mcp`
- Current pin (immutable pre-release): `v0.9.1-rc.1`, published 2026-07-30
- Pinned commit: `ee9833df2c72b750378273897ffec9410fc2c4f2`
- License: MIT
- Integrity manifest: [`code/runtimes/codebase-memory/artifact-manifest.json`](code/runtimes/codebase-memory/artifact-manifest.json)

The d-token installer ships only its restricted launcher and the legal text needed to explain the optional import. The upstream executable, installer, updater and native web UI are not included in the d-token installer. When a user chooses to import the pinned upstream archive, the d-token control plane verifies the archive and executable hashes before use and keeps the upstream license/notices with the imported component.

Superseded historical pin `v0.9.0` / `b637e3330c96cfe452da623db068c241aaa3ec01` (superseded 2026-08-06): retained only as the reproduction baseline for the user-reported v0.9.0 import failure; it is no longer the active import pin and is not offered for new imports.

## Reference-only projects

The projects below informed behavior studies or evaluation design. Their source code and binaries are not distributed as part of d-token unless a future notice explicitly states otherwise.

### Headroom

- Upstream: `headroomlabs-ai/headroom` (official remote: `https://github.com/headroomlabs-ai/headroom.git`)
- Pinned comparison tag/commit (unique current comparison pin, selected 2026-08-08 after a three-candidate drift audit): `v0.34.0` / `9fd5ae3d530b5d9eb9e9127e413235a5a3b2faf6` (released 2026-08-05; `pyproject.toml` version 0.34.0)
- Historical baseline (superseded by `v0.34.0`): `01df2452529a86c689cf226fecd5918cc5d19676` (main HEAD as fetched 2026-08-02; `pyproject.toml` version 0.33.0; kept only to explain earlier evidence)
- Previous release baseline (historical): `v0.33.0` / `28aa53dc7ca51e687cc719c3fe160f3be50c6570` (released 2026-07-29)
- Post-release delta (tracked, not pinned): main `7f6950be34e29304deae0fa5138b852491b092fe` (fetched 2026-08-08; `pyproject.toml` version still 0.34.0, unreleased)
- Historical comparison commit: `1725cd1f8376828849c0e48fd9008afa25d24fff`
- License: Apache License 2.0 (identical at all three candidate pins)
- Upstream NOTICE: since `v0.34.0` it lists the vendored dashboard assets (`headroom/dashboard/static/`: Tailwind CSS 3.4.17 MIT, htmx 1.9.10 Zero-Clause BSD, Alpine.js 3.13.3 MIT, added by `#2734`); no NOTICE change from `v0.34.0` to main. d-token does not distribute those upstream assets.

Versioned `headroom-*/` directories are local reference workspaces and are excluded from release source scope and desktop packages. d-token's Rust runtime is not a Python Headroom sidecar, does not silently install or launch Headroom, and does not depend on an external Headroom 8787 service. The CCR reversible-compression capability is a Headroom-internal component (not an independent upstream project); its provenance is covered by the Headroom pinned release above.

### lean-ctx (LeanCTX / Context Bus / Context Relay reference)

- Upstream: `yvgude/lean-ctx`
- Pinned release/commit: `v3.9.17` / `cf8fce6bb1759a845088988cca189beaf70b05a9`
- License: Apache License 2.0 (project-wide; root `LICENSE`, `README`, Rust `Cargo.toml` manifests and the Claude plugin `manifest.json` all declare Apache-2.0; `NOTICE` present)
- Subpackage exceptions (licensed MIT, listed separately, not part of the project-wide license): `clients/python` (`pyproject.toml`) and `rust/crates/grammar-addons/lua` (`Cargo.toml`)
- Local read-only research copy: `/Users/cc/Documents/d-token-external/lean-ctx/` (not distributed)
- Pin decision (2026-08-08): `v3.9.17` / `cf8fce6b…` is the **single reference pin**. Upstream `main` `95a1b7bc60a01479986af026c0c24451804993ba` (fetched 2026-08-08) was evaluated and kept as post-pin research only: 93 unreleased commits / 160 files / +11209 −2665 (ADR-001..013, new `lean-ctx-protocol` v0.1.0 crate, enterprise feature gating, in-flight r15–r18 "science" series, security fixes #1428/#1431–35/#1418/#1437) with no version bump and no LICENSE/NOTICE change (verified at both pins, including the unchanged MIT subpackage exceptions). Not a runtime dependency; no migration claimed.

lean-ctx informed behavior studies for the LeanCTX / Context Bus / Context Relay capability domain (`docs/PRD.md` P0.0.5). Its source code and binaries are not distributed as part of d-token. Headroom upstream removed its lean-ctx integration in `#2677` (merged 2026-07-31); d-token's LeanCTX alignment layer must be implemented as a d-token-owned Rust service and remains `unavailable`/`partial` until real cross-agent evidence exists.

### pxpipe

- Upstream: `teamchong/pxpipe`
- Pinned comparison commit: `7b7d6efa42926a73b1bd709b5f6d1af1ae679947`
- License: MIT

pxpipe informed public evaluation ideas around optical context representation. d-token's Rust renderer, request IR, patcher, routing and safety gates are independently implemented.

## Transitive package dependencies

JavaScript and Rust transitive dependency names, versions and declared licenses are generated from `package-lock.json` and Cargo metadata for each release candidate. The release workflow publishes:

- CycloneDX SBOM (`*.cdx.json`)
- dependency license inventory (`dependency-licenses.csv`)
- basic security scan report (`security-scan.md`)

These generated files describe the exact candidate dependency graph and are not a substitute for preserving upstream license texts required by applicable licenses.

For the detailed provenance and adaptation record used by maintainers, see [`docs/legal/THIRD_PARTY_NOTICES.md`](docs/legal/THIRD_PARTY_NOTICES.md).
