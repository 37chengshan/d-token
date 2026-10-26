# codebase-memory-mcp provenance boundary

This directory records the pinned, externally built headless artifact used by
the optional d-token Codebase Memory integration. The upstream C sources are
not copied into the d-token runtime and the upstream installer is never run.

- Upstream: `DeusData/codebase-memory-mcp`
- Release: immutable release `v0.10.8`
- Commit: `46ae198fc11cda80e817acbc5f5908d7c2de7032`
- Published: `2026-08-19T02:42:10Z` (GitHub immutable release)
- License: MIT
- Distribution: build/release preparation only; no runtime downloader or updater

### Version pin (2026-09-06)

The pin tracks upstream immutable release `v0.10.8` (commit
`46ae198fc11cda80e817acbc5f5908d7c2de7032`, published `2026-08-19T02:42:10Z`).
Its platform archive SHA-256 values come from the official `checksums.txt`
release asset. Its extracted executable SHA-256 values come from the official
`release-selection.tsv` and the release note security table. The desktop page
requires exactly this pinned tag/commit; any other archive or binary is rejected
by fail-closed verification and reported as untrusted. The previous pin
(`v0.9.1-rc.1`) was replaced because it collided with newer user-installed CBM
sessions through the shared daemon rendezvous.

`artifact-manifest.json` is the machine-readable source of archive and extracted
binary SHA-256 values for the supported Windows/macOS targets. Release
preparation must verify the archive digest and the Cosign blob bundle against
the pinned GitHub Actions workflow identity and OIDC issuer before extracting
the headless binary. The extracted `LICENSE` and upstream `THIRD_PARTY_NOTICES.md`
must accompany every desktop bundle. d-token verifies the exact extracted
headless executable SHA-256 before launch; no alternate signed, rebuilt or
repacked binary is accepted.

The product never exposes the upstream web UI. Agent-owned MCP sessions run
through `dtoken-codebase-memory-launcher`; d-token-owned graph viewing uses a
separate authorized read-only loopback projection.

### Local discovery and archive import

The desktop checks its configured component directory and common installed/PATH locations without executing discovered programs. Only manifest-verified binaries become usable. Users can select a downloaded official archive; the importer verifies the archive digest, extracts the same verified bytes with path and size limits, verifies the executable, and activates a fresh d-token-owned directory. External installations and prior imports are preserved. File locations and the extracted-directory fallback remain available in the page details. This does not add a runtime downloader or upstream installer.

### 2026-09-06: private daemon runtime isolation

Upstream `v0.10.8` documents `CBM_RUNTIME_DIR` as the parent directory for the
daemon/CLI rendezvous directory. `CBM_CACHE_DIR` still controls indexes,
settings and CBM logs, but it does not move the rendezvous. d-token therefore
derives a short private `/tmp/dtoken-cbm-runtime-<uid>/<cache-hash>` parent from
the approved project/role cache namespace and passes both `CBM_CACHE_DIR` and
`CBM_RUNTIME_DIR` to every owned upstream command. The short parent avoids Unix
socket length limits for ordinary macOS application-cache paths. Agent MCP
cache, graph UI cache and external user CBM sessions cannot join the same daemon
unless they intentionally share the same cache and runtime parent, which d-token
does not do.

The private runtime parent is created with owner-only permissions in a
d-token-owned temp namespace. d-token never stops external CBM processes,
deletes their locks or reads their private cache. Cache deletion removes the
derived private runtime directory only after d-token can prove no active UI/index
session or applied Agent integration still pins that cache. If upstream refuses
the supplied runtime parent, d-token reports the failed receipt instead of
falling back to the account-wide default.

Read-side scope is also bounded: the MCP proxy rejects caller-supplied project
selectors, filters `list_projects` to the approved canonical root, and the graph
UI refuses cache state that contains anything other than the single approved
project before resolving a root to the upstream project name.

Sources: [`docs/CONFIGURATION.md`](https://github.com/DeusData/codebase-memory-mcp/blob/46ae198fc11cda80e817acbc5f5908d7c2de7032/docs/CONFIGURATION.md), [`src/daemon/bootstrap.c`](https://github.com/DeusData/codebase-memory-mcp/blob/46ae198fc11cda80e817acbc5f5908d7c2de7032/src/daemon/bootstrap.c), [`src/daemon/ipc.c`](https://github.com/DeusData/codebase-memory-mcp/blob/46ae198fc11cda80e817acbc5f5908d7c2de7032/src/daemon/ipc.c), and [`tests/test_runtime_isolation_contract.sh`](https://github.com/DeusData/codebase-memory-mcp/blob/46ae198fc11cda80e817acbc5f5908d7c2de7032/tests/test_runtime_isolation_contract.sh). d-token analyzed these interfaces and did not copy upstream implementation code.
