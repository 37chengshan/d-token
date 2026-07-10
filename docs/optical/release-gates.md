# Optical Compression 发布门槛与完整性标准

## 发布门槛（Release Gates）

```text
Shadow mode changes no request body bytes.
Current user turn stays text.
Recent tail stays text.
Unknown model cannot active-transform.
Schema validator structure remains valid.
Open tool-call sequence cannot collapse.
Secret-like values are never sidecar duplicated.
Geometry is shared by estimator, renderer, and UI.
Cache-unstable candidates skip active mode.
Logs/exports contain no raw text/image/base64/exact values.
Renderer failure returns original body unchanged.
Patch validation preserves protocol structure.
Capability status is visible before active enable.
Calibration stores no raw content.
Over-budget inputs fail closed or pass-through.
```

## 完整性标准（Completeness Criteria）

```text
Every active path has a fail-closed rule.
Every public output has a raw-content forbidden list.
Every candidate action has a skip/apply reason.
Every model/endpoint active path depends on explicit capability.
Every image estimate depends on RenderGeometryProfile.
Every raw text reference is request-local and non-serializable.
Every history compression path depends on closed tool boundary and cache stability.
Every schema slimming path preserves validator structure.
Every sidecar path excludes secret-like values.
Every budget overflow path avoids active mutation.
```

## 实施顺序

```text
Phase A: Plan-only foundation (A1-A7) ✅
Phase B: API visibility (/preview, /status, /capabilities)
Phase C: Desktop UI (Optical Lab)
Phase D: Renderer and patcher (feature flag)
Phase E: History compression
```
