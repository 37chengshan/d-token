# Optical Compression 数据结构汇总

## 公开枚举（dtoken-types）

```rust
OpticalMode: Off | ShadowPreview | ToolResultsOnly | HistoryOnly | FullExperimental
ProviderFamily: Anthropic | OpenAi | Other
OpticalEndpoint: AnthropicMessages | OpenAiChatCompletions | OpenAiResponses | Unsupported
CapabilityStatus: Supported | Limited | ShadowOnly | Blocked | Unknown
VisionExactnessClass: SummaryOnly | TechnicalReadable | ExactStringUnsafe | Unknown
VisionTokenFormula: PixelAreaDivisor { pixels_per_token, safety_margin_percent } | Unknown
OpticalApplicabilityReason: Eligible | GlobalDisabled | RouteDisabled | UnsupportedMethod | UnsupportedPath | EmptyBody | UnsupportedProtocol | UnsupportedModel | UnknownModelShadowOnly | OverBudget
OpticalPlanStatus: Disabled | Ineligible | ValidatedShadowPlan | ValidatedActivePlan | FailedClosed
OpticalBucket: StaticSlab | Reminder | ToolResultJson | ToolResultLog | ToolResultProse | ToolResultTable | AnthropicHistory | GptHistory | SchemaAnnotation
```

## 公开结构（dtoken-types）

```rust
OpticalModelCapability {
  provider_family, provider_id, model_base, endpoint, status, max_mode,
  supports_image_input, supports_anthropic_image_block, supports_openai_image_url,
  supports_openai_input_image, supports_data_url, max_images_per_request,
  max_image_width_px, max_image_height_px, vision_token_formula, exactness_class
}

OpticalApplicabilityInput {
  method, endpoint_path, body_bytes, protocol, provider_id, route_id, model,
  global_enabled, route_enabled
}

OpticalApplicabilityDecision {
  eligible_for_shadow, eligible_for_active, reason, normalized_model_base, endpoint
}

OpticalGlobalPolicy {
  enabled=false, default_mode=ShadowPreview, allow_active=false,
  max_images_per_request=10, max_candidate_chars_per_request=512000,
  max_plan_time_ms=50, max_render_time_ms=1500, public_summary_only=true
}

RouteOpticalPolicy {
  enabled=false, mode=Off, allow_tool_result=false, allow_history=false,
  allow_schema_annotations=false, allow_system_static_slab=false,
  min_net_saved_tokens=1024, confirmed_experimental_lossy=false
}

OpticalGateDecision { name, passed, reason }
OpticalPlanCounters { candidate_blocks, preserved_blocks, skipped_blocks, exact_spans, secret_spans, estimated_images }
OpticalBucketSummary { bucket, blocks, chars, estimated_net_saved_tokens }
CachePublicSummary { stable_prefix_digest_present, frozen_sections, cache_tokens_at_risk }
OpticalEconomicsPublicSummary { text_tokens_before, image_tokens_after, sidecar_tokens, cache_burn_tokens, net_saved_tokens }
OpticalPlanPublicSummary { plan_id, mode, final_status, gates, counters, buckets, cache, economics, warnings }
```

## 内部结构（dtoken-compress/optical）

```rust
RenderGeometryProfile { profile_id, provider_family, width_px, max_height_px, cols, readable_rows, readable_chars_per_image }

TextRef { id: TextRefId, len_chars, len_bytes, digest, sensitivity }  // NOT Serialize
TextArena { request_id, entries }  // NOT Serialize
TextSensitivity: NormalInternal | ExactInternal | SecretInternal | RenderCandidateInternal

VolatilityPlan { stable_segments, volatile_segments, stable_digest, volatile_chars, churning_static_tags }
SchemaSlimPlan { tool_count, schemas_considered, schemas_slimmed, annotations_moved_chars, validator_chars_before, validator_chars_after, annotation_blocks, skipped }
ConversationInventory { protocol, endpoint, turns, blocks, current_user_turn, recent_tail_start }
RiskTier: RenderSafe | ExactTextRequired | SecretNoDuplicate | StructuralUnsafe | BlockUnsafe
AnchorMap { anchors, sidecar_policy, render_text_ref, sidecar_text_ref }
BlockEconomicsPlan { bucket, text_tokens_before, image_tokens_after, sidecar_tokens, cache_burn_image_side, cache_burn_text_side, output_risk_tokens, latency_risk_level, net_saved_tokens, profitable, reason }
CandidatePlan { block_id, bucket, block_kind, path, chars_original, chars_render_candidate, risk, anchors, reflow, paging, economics, action, reason }
CandidateAction: PreserveText | ShadowEstimateOnly | RenderAsImage | RenderWithTextSidecar | ... | Skip*
OpticalTransformPlan { plan_id, mode, capability, applicability, volatility, schema_slim, inventory, semantic_priority, boundaries, cache, candidates, economics, render_plan, patch_plan, validation, final_status, gates }
```

## 公开 summary 禁止字段

```text
raw block text, schema annotation text, sidecar exact text, rendered text,
PNG bytes, base64, request body, response body, exact values, secret values
```
