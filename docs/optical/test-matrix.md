# Optical Compression 测试矩阵

## Gate 测试（applicability + capability + render_geometry）

```text
applicability:
- POST /v1/messages Anthropic shadow
- GET /v1/messages ineligible
- empty body ineligible
- unknown model shadow-only
- unsupported path ineligible
- over-budget request not active

capability:
- unknown model cannot active-transform
- blocked provider cannot active-transform
- supported Anthropic image block allows tool_result active

render_geometry:
- profile has nonzero dimensions
- readable_chars_per_image derived from cols/rows
- estimator consumes profile rather than constants
```

## Plan 层测试

```text
volatility:
- environment block becomes volatile
- stable digest ignores volatile segment
- churning stable tag becomes warning

schema_slimmer:
- property named description is preserved
- required remains valid
- enum/const/type/$ref preserved
- annotation chars moved to annotation block
- unknown schema context preserves text

inventory:
- Anthropic tool_result candidate detected
- recent tail protected
- system prompt protected
- existing images opaque

semantic_priority:
- current user AlwaysText
- latest collapsed user pointer generated for imaged history
- open tool state AlwaysText

risk:
- UUID/hash/path become ExactTextRequired
- bearer/cookie/API-key become SecretNoDuplicate
- diff becomes StructuralUnsafe
- dense exact spans skip block
- anchor count over budget skips block

economics:
- sparse prose unprofitable
- dense log profitable
- cache burn can flip decision
- history uses amortized gate

validator:
- shadow body unchanged
- active rejects unknown model
- active rejects cache-unstable candidate
- active rejects secret sidecar duplication
- patch path outside candidate rejected
```

## TextArena 测试

```text
- text_ref_debug_does_not_print_raw_text
- text_arena_tracks_request_local_entries
```

## 当前通过状态

```text
cargo test -p dtoken-types optical   → 2 passed, 0 failed
cargo test -p dtoken-compress optical → 12 passed, 0 failed
```
