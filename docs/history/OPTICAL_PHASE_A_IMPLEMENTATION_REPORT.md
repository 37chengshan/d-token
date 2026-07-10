# Optical Compression Phase A Implementation Report

## 1. Scope

This report describes the current implementation status for Optical Compression Phase A, corresponding to the A1-A7 coding target in `docs/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`.

Phase A goal:

```text
A1 dtoken-types public optical enums/stats/policy/public summary
A2 dtoken-compress optical module skeleton
A3 capability.rs
A4 applicability.rs
A5 render_geometry.rs
A6 TextRef/TextArena raw text containment
A7 unit tests for serde/defaults/exact path/model applicability/capability/redaction
```

Phase A explicitly does not include:

```text
PNG renderer
UI active switch
request mutation
OpenAI active transform
history active transform
persistent raw recovery cache
```

## 2. Branch

Current branch reported by CodexPro:

```text
optical
```

The workspace already contains many unrelated modified and untracked files. This Phase A work intentionally touched only the optical planning/type files and the implementation report, except where a crate-root export is still required manually.

## 3. Completed Work

### A1. dtoken-types public optical enums/stats/policy/public summary

Implemented in:

```text
code/crates/dtoken-types/src/lib.rs
```

Added public optical data structures:

```text
OpticalMode
ProviderFamily
OpticalEndpoint
CapabilityStatus
VisionExactnessClass
VisionTokenFormula
OpticalModelCapability
OpticalApplicabilityInput
OpticalApplicabilityDecision
OpticalApplicabilityReason
OpticalGlobalPolicy
RouteOpticalPolicy
OpticalPlanStatus
OpticalGateDecision
OpticalPlanCounters
OpticalBucket
OpticalBucketSummary
CachePublicSummary
OpticalEconomicsPublicSummary
OpticalPlanPublicSummary
```

Important defaults:

```text
OpticalGlobalPolicy.enabled = false
OpticalGlobalPolicy.default_mode = shadow_preview
OpticalGlobalPolicy.allow_active = false
OpticalGlobalPolicy.public_summary_only = true
RouteOpticalPolicy.enabled = false
RouteOpticalPolicy.mode = off
RouteOpticalPolicy.confirmed_experimental_lossy = false
```

### A2. dtoken-compress optical module skeleton

Implemented files:

```text
code/crates/dtoken-compress/src/optical/mod.rs
code/crates/dtoken-compress/src/optical/capability.rs
code/crates/dtoken-compress/src/optical/applicability.rs
code/crates/dtoken-compress/src/optical/render_geometry.rs
code/crates/dtoken-compress/src/optical/text_arena.rs
```

`mod.rs` re-exports the Phase A public planner functions and types.

Pending manual crate-root export:

```rust
pub mod optical;
```

This must be added to the first line of:

```text
code/crates/dtoken-compress/src/lib.rs
```

Reason: CodexPro safe editing currently blocks edits to that file because its existing saved-token calculation line is flagged by the scanner.

### A3. capability.rs

Implemented in:

```text
code/crates/dtoken-compress/src/optical/capability.rs
```

Functions:

```text
normalize_model_base
provider_family_for_protocol
resolve_model_capability
```

Current policy:

```text
Anthropic + /v1/messages + claude-fable-5 => Supported + ToolResultsOnly
Anthropic + /v1/messages + other claude-* => Limited + ShadowPreview
OpenAI chat/responses endpoints => ShadowOnly
Unsupported endpoint => Blocked
Unknown model/provider => Unknown
```

This keeps active transformation conservative and model-gated.

### A4. applicability.rs

Implemented in:

```text
code/crates/dtoken-compress/src/optical/applicability.rs
```

Functions:

```text
resolve_endpoint
evaluate_applicability
```

Covered decisions:

```text
empty body => ineligible
non-POST => ineligible
unsupported path => ineligible
global disabled => shadow only
route disabled => shadow only
supported Anthropic model => shadow + active eligible
unknown model => shadow only
```

### A5. render_geometry.rs

Implemented in:

```text
code/crates/dtoken-compress/src/optical/render_geometry.rs
```

Functions/types:

```text
RenderGeometryProfile
anthropic_dense_v1
estimate_image_tokens
geometry_for_capability
```

Current profile:

```text
profile_id = anthropic_dense_v1
width_px = 1568
max_height_px = 728
cols = 312
readable_rows = 89
readable_chars_per_image = 27768
```

This is intentionally a minimal profile-driven estimator. Full renderer geometry and glyph handling remain later work.

### A6. TextRef/TextArena raw text containment

Implemented in:

```text
code/crates/dtoken-compress/src/optical/text_arena.rs
```

Types:

```text
TextArena
TextRef
TextRefId
TextSensitivity
```

Containment properties:

```text
TextRef does not implement Serialize.
TextArena does not implement Serialize.
TextRef Debug prints id, lengths, digest prefix, and sensitivity only.
TextArena Debug prints request id and entry count only.
Raw text stays request-local inside TextArena.
TextSensitivity distinguishes normal, exact, secret, and render-candidate internal text.
```

### A7. Unit and integration tests

Added:

```text
code/crates/dtoken-types/tests/optical_types.rs
code/crates/dtoken-compress/tests/optical_phase_a.rs
```

`optical_types.rs` covers:

```text
OpticalGlobalPolicy default safety
RouteOpticalPolicy default off state
OpticalPlanPublicSummary JSON redaction surface
```

`optical_phase_a.rs` covers:

```text
model normalization
supported Anthropic capability
unsupported/unsafe applicability paths
profile-driven geometry
TextArena debug redaction and request-local raw text lookup
```

## 4. Not Completed / Blocked

### 4.1 Crate-root optical export

Required manual patch:

```rust
pub mod optical;
```

Add this as the first line of:

```text
code/crates/dtoken-compress/src/lib.rs
```

Without this, the `dtoken-compress` optical integration test cannot import `dtoken_compress::optical`.

### 4.2 Existing saved-token calculation line in dtoken-compress/src/lib.rs

CodexPro displays a scanner-placeholder around the current saved-token calculation line in:

```text
code/crates/dtoken-compress/src/lib.rs
```

Inspect this file locally. The intended calculation should be equivalent to:

```text
saved tokens = original tokens minus compressed tokens, using saturating subtraction
```

This is separate from optical Phase A, but it currently prevents CodexPro from editing the file.

### 4.3 Local cargo verification

Verification could not run in CodexPro because `cargo` is not available in the current shell:

```text
/bin/bash: line 1: cargo: command not found
```

Therefore, compilation and tests still need to be run locally.

## 5. Required Local Tests

After applying the manual crate-root export, run:

```bash
cd code
cargo test -p dtoken-types optical
cargo test -p dtoken-compress optical
```

Recommended broader checks:

```bash
cd code
cargo test -p dtoken-types
cargo test -p dtoken-compress
cargo test -p dtoken-router
cargo test -p dtoken-core-api
```

If formatting tools are available:

```bash
cd code
cargo fmt --all
```

## 6. Expected Failure If Manual Patch Is Missing

If `pub mod optical;` is not added to `code/crates/dtoken-compress/src/lib.rs`, the integration test will fail because this path is unavailable:

```rust
dtoken_compress::optical
```

This failure is expected until the manual patch is applied.

## 7. Reference Plan Documents

Primary reference:

```text
docs/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md
```

Relevant sections from that plan:

```text
0. Product boundary
1. Overall architecture
3. Configuration Schema
4. CapabilityResolver
5. ApplicabilityGate
6. RenderGeometryProfile
7. TextRef and Raw Text Containment
20. OpticalTransformPlan
22. PlanValidator
29. Test Matrix
30. Implementation Order
31. Release Gates
32. Next Coding Task
33. Completeness Criteria
```

Manual patch note:

```text
.ai-bridge/optical-phase-a-manual-patch.md
```

## 8. Current Status

Phase A implementation status:

```text
A1 completed
A2 completed, except manual crate-root export
A3 completed
A4 completed
A5 completed as minimal profile-driven estimator
A6 completed
A7 completed, pending local cargo execution
```

Readiness statement:

```text
The Phase A source files and tests are in place. The remaining work is not algorithmic; it is local integration verification: add the crate-root export, inspect the pre-existing saved-token calculation line, and run cargo tests locally.
```
