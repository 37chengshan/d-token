export type OpticalSlice<T> = {
  ok: boolean;
  source: string;
  error: string;
  data: T | null;
};

export type OpticalStatus = {
  schema_version?: number;
  enabled?: boolean;
  configured_mode?: string;
  effective_mode?: string;
  default_mode?: string;
  allow_active?: boolean;
  active_reachable?: boolean;
  active_execution_observed?: boolean;
  public_summary_only?: boolean;
  fail_closed?: boolean;
  renderer_ready?: boolean;
  patcher_ready?: boolean;
  post_validator_ready?: boolean;
  signed_accounting_ready?: boolean;
  active_direct_routes?: number;
  verified_attested_capabilities?: number;
  rejected_capabilities?: number;
  config_issue_count?: number;
};

export type OpticalCapabilities = {
  shadow_preview_supported?: boolean;
  active_transform_enabled?: boolean;
  max_mode?: string;
  unknown_model_status?: string;
  unsupported_path_status?: string;
  public_summary_only?: boolean;
  fail_closed?: boolean;
  direct_anthropic_upstream_supported?: boolean;
  streaming_passthrough_supported?: boolean;
  deterministic_png_renderer?: boolean;
  artifact_post_validation?: boolean;
  signed_multimodal_accounting?: boolean;
  capability_attestation_required?: boolean;
};

export type OpticalLifecycle = {
  schema_version?: number;
  configured_snapshots?: number;
  active_snapshots?: number;
  revoked_snapshots?: number;
  superseded_snapshots?: number;
  verified_rules?: number;
  rejected_rules?: number;
  trusted_authorities?: number;
  eval_artifacts?: number;
  passing_eval_artifacts?: number;
  attestation_required?: boolean;
};

export type OpticalEventMetadata = {
  kind?: string;
  reason_code?: string | null;
  final_status?: string;
  mode?: string;
  candidate_blocks?: number;
  preserved_blocks?: number;
  skipped_blocks?: number;
  estimated_images?: number;
  patched_blocks?: number;
  net_saved_tokens?: number;
  graph_nodes?: number;
  graph_edges?: number;
  closed_tool_results?: number;
  orphan_tool_results?: number;
  open_tool_uses?: number;
  route_policy_found?: boolean;
  verified_rules?: number;
  rejected_rules?: number;
  raw_available?: boolean;
};

export type OpticalEvent = {
  event_id?: string;
  timestamp_ms?: number;
  stage?: string;
  status?: string;
  metadata?: OpticalEventMetadata;
};

export type OpticalObservabilitySnapshot = {
  ok: boolean;
  complete: boolean;
  partial: boolean;
  source: string;
  collected_at: number;
  service_url?: string;
  error?: string;
  status: OpticalSlice<OpticalStatus>;
  capabilities: OpticalSlice<OpticalCapabilities>;
  lifecycle: OpticalSlice<OpticalLifecycle>;
  events: OpticalSlice<OpticalEvent[]>;
};

export type OpticalPlanSummary = {
  plan_id?: string;
  mode?: string;
  final_status?: string;
  gates?: Array<{ name?: string; passed?: boolean; reason?: string }>;
  counters?: {
    candidate_blocks?: number;
    preserved_blocks?: number;
    skipped_blocks?: number;
    estimated_images?: number;
  };
  economics?: {
    net_saved_tokens?: number;
    estimated_input_tokens?: number;
    estimated_vision_tokens?: number;
  };
  warnings?: string[];
};

export type OpticalPreviewResult = {
  ok: boolean;
  source: string;
  error: string;
  data: OpticalPlanSummary | null;
};

/**
 * Optional shadow-preview request routing. Only an opaque route_id is passed
 * through — provider/model stay null so the request never carries credential
 * or model-sensitive values beyond what the public summary contract allows.
 */
export type OpticalPreviewRequestOptions = {
  route_id?: string;
};

export type OpticalConfigDraft = {
  global: {
    enabled: boolean;
    default_mode: "off" | "shadow_preview";
  };
  routes: Array<{
    route_id: string;
    enabled: boolean;
    mode: "off" | "shadow_preview";
    allow_tool_result: boolean;
    allow_history: boolean;
    confirmed_experimental_lossy: boolean;
  }>;
};

export type OpticalConfigEditorSnapshot = {
  ok: boolean;
  source: string;
  error: string;
  service_running: boolean;
  service_state: "running" | "unmanaged_running" | "stopped" | "unknown";
  active_config_exists: boolean;
  active_config_path?: string;
  route_count: number;
  routes: Array<{ route_id: string; label?: string }>;
  draft: OpticalConfigDraft;
  issues: string[];
  allow_active_forced_off?: boolean;
};

export type OpticalConfigReview = {
  ok: boolean;
  source: string;
  error?: string;
  target?: string;
  before_sha256?: string;
  after_sha256?: string;
  diff?: string;
  risk_level?: string;
  requires_explicit_confirmation?: boolean;
  restart_required?: boolean;
  allow_active?: boolean;
};

export type OpticalConfigApplyResult = {
  ok: boolean;
  source: string;
  error?: string;
  summary?: string;
  transaction_id?: string;
  rollback_id?: string;
  config_applied?: boolean;
  config_restored?: boolean;
  runtime_verified?: boolean;
  restarted?: boolean;
  restart_summary?: string;
  allow_active?: boolean;
};

export const OPTICAL_EVENT_RENDER_LIMIT = 50;

export const OPTICAL_SAMPLE_BODY = {
  messages: [
    {
      role: "user",
      content: "Summarize the closed tool result without secrets.",
    },
    {
      role: "assistant",
      content: [
        {
          type: "tool_use",
          id: "toolu_sample_1",
          name: "read_file",
          input: { path: "README.md" },
        },
      ],
    },
    {
      role: "user",
      content: [
        {
          type: "tool_result",
          tool_use_id: "toolu_sample_1",
          content: "Closed tool result for shadow planning only.",
        },
      ],
    },
  ],
};
