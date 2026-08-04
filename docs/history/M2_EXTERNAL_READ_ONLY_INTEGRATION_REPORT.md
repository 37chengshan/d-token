# M2 External Read-only Integration Report

Status: implemented as read-only integration surfaces with fixture/fake validation.

## Scope

M2 adds the API and adapter surfaces needed to connect to existing RTK and Headroom installs only when the user explicitly enables shadow/external mode. Per the current local instruction, no running local Headroom service was contacted during validation.

## Implemented

1. RTK gain, discover, and session report structures.
2. RTK discover/session text parsing.
3. Headroom stats history and Prometheus metrics fixture surfaces.
4. Layered savings report shape that keeps RTK and Headroom savings separately attributed.
5. JSONL event ledger persistence for structured, aggregate-only events.
6. Core API endpoints now return fixture-driven M2 data instead of pure static mocks.
7. `/compatibility/agents` exposes the RTK + Headroom supported-agent union.
8. `/protocol-injection` exposes OpenAI-compatible and Anthropic-compatible injection plans without touching live services.
9. Headroom external read-only HTTP paths are implemented behind explicit `DTOKEN_HEADROOM_BASE_URL`; default fixture mode never uses them.

## Verification

1. `cargo check --workspace` passed.
2. Frontend and API fixture data paths compile.
3. PowerShell fixture smoke reached structure and cargo-check gates.
4. No real Headroom process, endpoint, or local service was queried.

## Multidimensional Review

Product: supports the M2 goal of read-only external integration while preserving an explicit opt-in boundary.

Engineering: parser and report types are now reusable by both API and future GUI consumers.

Security/privacy: event ledger tests assert aggregate-only metadata and no full prompt field.

Operations: local service verification is intentionally not performed on this machine because the user instructed not to touch the running local service.

Risk: real HTTP integration against Headroom health/stats endpoints is represented by API shape and fixture data, but live endpoint behavior still needs validation in a dedicated sandbox.

Coverage review: the compatibility matrix now includes RTK-only, Headroom-only, shared, planned, and library-only agents, with macOS, Windows native, and Windows WSL support levels.

No-touch review: the Headroom HTTP client is inert unless the operator sets `DTOKEN_HEADROOM_BASE_URL` and chooses a non-fixture profile. It was not exercised against the user's running local service.
