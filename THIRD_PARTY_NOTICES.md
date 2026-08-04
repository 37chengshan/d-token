# d-token Third-Party Notices

Last updated: 2026-08-02  
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
- Pinned release: `v0.9.0`
- Pinned commit: `b637e3330c96cfe452da623db068c241aaa3ec01`
- License: MIT
- Integrity manifest: [`code/runtimes/codebase-memory/artifact-manifest.json`](code/runtimes/codebase-memory/artifact-manifest.json)

The d-token installer ships only its restricted launcher and the legal text needed to explain the optional import. The upstream executable, installer, updater and native web UI are not included in the d-token installer. When a user chooses to import the pinned upstream archive, the d-token control plane verifies the archive and executable hashes before use and keeps the upstream license/notices with the imported component.

## Reference-only projects

The projects below informed behavior studies or evaluation design. Their source code and binaries are not distributed as part of d-token unless a future notice explicitly states otherwise.

### Headroom

- Upstream: `headroomlabs-ai/headroom`
- Pinned comparison tag/commit: `v0.33.0` / `28aa53dc7ca51e687cc719c3fe160f3be50c6570`
- Historical comparison commit: `1725cd1f8376828849c0e48fd9008afa25d24fff`
- License: Apache License 2.0

Versioned `headroom-*/` directories are local reference workspaces and are excluded from release source scope and desktop packages. d-token's Rust runtime is not a Python Headroom sidecar and does not silently install or launch Headroom.

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
