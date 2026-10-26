import { invokeNative, type NativeActionResult } from "../native";
import type {
  OpticalCapabilities,
  OpticalConfigApplyResult,
  OpticalConfigDraft,
  OpticalConfigEditorSnapshot,
  OpticalConfigReview,
  OpticalEvent,
  OpticalLifecycle,
  OpticalObservabilitySnapshot,
  OpticalPlanSummary,
  OpticalPreviewRequestOptions,
  OpticalPreviewResult,
  OpticalSlice,
  OpticalStatus,
} from "./types";
import { OPTICAL_EVENT_RENDER_LIMIT } from "./types";
import { clampOpticalMode, defaultOpticalDraft, normalizeOpticalDraft } from "./configDraft";

function asObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function asBool(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function pickStatus(data: unknown): OpticalStatus | null {
  const object = asObject(data);
  if (!object) return null;
  return {
    schema_version: asNumber(object.schema_version),
    enabled: asBool(object.enabled),
    configured_mode: asString(object.configured_mode),
    effective_mode: asString(object.effective_mode),
    default_mode: asString(object.default_mode),
    allow_active: asBool(object.allow_active),
    active_reachable: asBool(object.active_reachable ?? object.allow_active),
    active_execution_observed: asBool(object.active_execution_observed),
    public_summary_only: asBool(object.public_summary_only),
    fail_closed: asBool(object.fail_closed),
    renderer_ready: asBool(object.renderer_ready),
    patcher_ready: asBool(object.patcher_ready),
    post_validator_ready: asBool(object.post_validator_ready),
    signed_accounting_ready: asBool(object.signed_accounting_ready),
    active_direct_routes: asNumber(object.active_direct_routes),
    verified_attested_capabilities: asNumber(object.verified_attested_capabilities),
    rejected_capabilities: asNumber(object.rejected_capabilities),
    config_issue_count: asNumber(object.config_issue_count),
  };
}

function pickCapabilities(data: unknown): OpticalCapabilities | null {
  const object = asObject(data);
  if (!object) return null;
  return {
    shadow_preview_supported: asBool(object.shadow_preview_supported),
    active_transform_enabled: asBool(object.active_transform_enabled),
    max_mode: asString(object.max_mode),
    unknown_model_status: asString(object.unknown_model_status),
    unsupported_path_status: asString(object.unsupported_path_status),
    public_summary_only: asBool(object.public_summary_only),
    fail_closed: asBool(object.fail_closed),
    direct_anthropic_upstream_supported: asBool(object.direct_anthropic_upstream_supported),
    streaming_passthrough_supported: asBool(object.streaming_passthrough_supported),
    deterministic_png_renderer: asBool(object.deterministic_png_renderer),
    artifact_post_validation: asBool(object.artifact_post_validation),
    signed_multimodal_accounting: asBool(object.signed_multimodal_accounting),
    capability_attestation_required: asBool(object.capability_attestation_required),
  };
}

function pickLifecycle(data: unknown): OpticalLifecycle | null {
  const object = asObject(data);
  if (!object) return null;
  return {
    schema_version: asNumber(object.schema_version),
    configured_snapshots: asNumber(object.configured_snapshots),
    active_snapshots: asNumber(object.active_snapshots),
    revoked_snapshots: asNumber(object.revoked_snapshots),
    superseded_snapshots: asNumber(object.superseded_snapshots),
    verified_rules: asNumber(object.verified_rules),
    rejected_rules: asNumber(object.rejected_rules),
    trusted_authorities: asNumber(object.trusted_authorities),
    eval_artifacts: asNumber(object.eval_artifacts),
    passing_eval_artifacts: asNumber(object.passing_eval_artifacts),
    attestation_required: asBool(object.attestation_required),
  };
}

function pickEvent(value: unknown): OpticalEvent | null {
  const object = asObject(value);
  if (!object) return null;
  const metadataObject = asObject(object.metadata) ?? {};
  return {
    event_id: asString(object.event_id),
    timestamp_ms: asNumber(object.timestamp_ms),
    stage: asString(object.stage),
    status: asString(object.status),
    metadata: {
      kind: asString(metadataObject.kind),
      reason_code: asString(metadataObject.reason_code) ?? null,
      final_status: asString(metadataObject.final_status),
      mode: asString(metadataObject.mode),
      candidate_blocks: asNumber(metadataObject.candidate_blocks),
      preserved_blocks: asNumber(metadataObject.preserved_blocks),
      skipped_blocks: asNumber(metadataObject.skipped_blocks),
      estimated_images: asNumber(metadataObject.estimated_images),
      patched_blocks: asNumber(metadataObject.patched_blocks),
      net_saved_tokens: asNumber(metadataObject.net_saved_tokens),
      graph_nodes: asNumber(metadataObject.graph_nodes),
      graph_edges: asNumber(metadataObject.graph_edges),
      closed_tool_results: asNumber(metadataObject.closed_tool_results),
      orphan_tool_results: asNumber(metadataObject.orphan_tool_results),
      open_tool_uses: asNumber(metadataObject.open_tool_uses),
      route_policy_found: asBool(metadataObject.route_policy_found),
      verified_rules: asNumber(metadataObject.verified_rules),
      rejected_rules: asNumber(metadataObject.rejected_rules),
      raw_available: asBool(metadataObject.raw_available),
    },
  };
}

function pickEvents(data: unknown): OpticalEvent[] | null {
  if (!Array.isArray(data)) return null;
  return data
    .map(pickEvent)
    .filter((event): event is OpticalEvent => event !== null)
    .slice(0, OPTICAL_EVENT_RENDER_LIMIT);
}

function pickPlan(data: unknown): OpticalPlanSummary | null {
  const object = asObject(data);
  if (!object) return null;
  const counters = asObject(object.counters);
  const economics = asObject(object.economics);
  const gates = Array.isArray(object.gates)
    ? object.gates.map((gate) => {
        const item = asObject(gate) ?? {};
        return {
          name: asString(item.name),
          passed: asBool(item.passed),
          reason: asString(item.reason),
        };
      })
    : undefined;
  const warnings = Array.isArray(object.warnings)
    ? object.warnings.filter((item): item is string => typeof item === "string").slice(0, 20)
    : undefined;
  return {
    plan_id: asString(object.plan_id),
    mode: asString(object.mode),
    final_status: asString(object.final_status),
    gates,
    counters: counters
      ? {
          candidate_blocks: asNumber(counters.candidate_blocks),
          preserved_blocks: asNumber(counters.preserved_blocks),
          skipped_blocks: asNumber(counters.skipped_blocks),
          estimated_images: asNumber(counters.estimated_images),
        }
      : undefined,
    economics: economics
      ? {
          net_saved_tokens: asNumber(economics.net_saved_tokens),
          estimated_input_tokens: asNumber(economics.estimated_input_tokens),
          estimated_vision_tokens: asNumber(economics.estimated_vision_tokens),
        }
      : undefined,
    warnings,
  };
}

function pickSlice<T>(
  raw: unknown,
  pick: (data: unknown) => T | null,
): OpticalSlice<T> {
  const object = asObject(raw);
  if (!object) {
    return { ok: false, source: "unavailable", error: "missing_slice", data: null };
  }
  return {
    ok: Boolean(object.ok),
    source: asString(object.source) ?? "unavailable",
    error: asString(object.error) ?? "",
    data: pick(object.data),
  };
}

export function normalizeOpticalObservability(raw: unknown): OpticalObservabilitySnapshot {
  const object = asObject(raw) ?? {};
  return {
    ok: Boolean(object.ok),
    complete: Boolean(object.complete),
    partial: Boolean(object.partial),
    source: asString(object.source) ?? "unavailable",
    collected_at: asNumber(object.collected_at) ?? 0,
    service_url: asString(object.service_url),
    error: asString(object.error),
    status: pickSlice(object.status, pickStatus),
    capabilities: pickSlice(object.capabilities, pickCapabilities),
    lifecycle: pickSlice(object.lifecycle, pickLifecycle),
    events: pickSlice(object.events, pickEvents),
  };
}

export function normalizeOpticalPreview(raw: unknown): OpticalPreviewResult {
  const object = asObject(raw) ?? {};
  return {
    ok: Boolean(object.ok),
    source: asString(object.source) ?? "unavailable",
    error: asString(object.error) ?? "",
    data: pickPlan(object.data),
  };
}

export async function loadOpticalObservability(): Promise<OpticalObservabilitySnapshot> {
  const raw = await invokeNative<unknown>("get_optical_observability");
  return normalizeOpticalObservability(raw);
}

export async function previewOpticalPlan(
  body: unknown,
  options?: OpticalPreviewRequestOptions,
): Promise<OpticalPreviewResult> {
  // Match OpticalPreviewRequest: endpoint_path + optional protocol + body only.
  // Do not send raw PNG/base64 or secrets; UI only submits operator-provided JSON.
  // An optional opaque route_id may be passed for route-scoped planning; provider
  // and model stay null (unknown-model shadow-only unless the route supplies policy).
  const request = {
    endpoint_path: "/v1/messages",
    protocol: "anthropic",
    provider_id: null,
    route_id: options?.route_id ?? null,
    model: null,
    body,
  };
  if (body === undefined || body === null) {
    return {
      ok: false,
      source: "unavailable",
      error: "preview body is required",
      data: null,
    };
  }
  const raw = await invokeNative<unknown>("preview_optical_plan", { request });
  return normalizeOpticalPreview(raw);
}

function pickConfigDraft(raw: unknown, knownRoutes: Array<{ route_id: string; label?: string }>): OpticalConfigDraft {
  const object = asObject(raw) ?? {};
  const global = asObject(object.global) ?? {};
  const routesRaw = Array.isArray(object.routes) ? object.routes : [];
  const normalized = normalizeOpticalDraft(
    {
      global: {
        enabled: Boolean(global.enabled),
        default_mode: clampOpticalMode(global.default_mode),
      },
      routes: routesRaw.map((item) => {
        const route = asObject(item) ?? {};
        return {
          route_id: asString(route.route_id) ?? "",
          enabled: Boolean(route.enabled),
          mode: clampOpticalMode(route.mode),
          allow_tool_result: Boolean(route.allow_tool_result),
          allow_history: Boolean(route.allow_history),
          confirmed_experimental_lossy: Boolean(route.confirmed_experimental_lossy),
        };
      }),
    },
    knownRoutes,
  );
  if (normalized.ok) return normalized.draft;
  return defaultOpticalDraft(knownRoutes);
}

export function normalizeOpticalConfigEditor(raw: unknown): OpticalConfigEditorSnapshot {
  const object = asObject(raw) ?? {};
  const routes: Array<{ route_id: string; label?: string }> = Array.isArray(object.routes)
    ? object.routes.flatMap((item) => {
        const route = asObject(item) ?? {};
        const route_id = asString(route.route_id);
        if (!route_id) return [];
        const label = asString(route.label);
        return label ? [{ route_id, label }] : [{ route_id }];
      })
    : [];
  return {
    ok: Boolean(object.ok),
    source: asString(object.source) ?? "unavailable",
    error: asString(object.error) ?? "",
    service_running: Boolean(object.service_running),
    service_state:
      object.service_state === "running"
      || object.service_state === "unmanaged_running"
      || object.service_state === "stopped"
        ? object.service_state
        : "unknown",
    active_config_exists: Boolean(object.active_config_exists),
    active_config_path: asString(object.active_config_path),
    route_count: asNumber(object.route_count) ?? routes.length,
    routes,
    draft: pickConfigDraft(object.draft, routes),
    issues: Array.isArray(object.issues)
      ? object.issues.filter((item): item is string => typeof item === "string")
      : [],
    allow_active_forced_off: object.allow_active_forced_off !== false,
  };
}

export function normalizeOpticalConfigReview(raw: unknown): OpticalConfigReview {
  const object = asObject(raw) ?? {};
  return {
    ok: Boolean(object.ok),
    source: asString(object.source) ?? "unavailable",
    error: asString(object.error),
    target: asString(object.target),
    before_sha256: asString(object.before_sha256),
    after_sha256: asString(object.after_sha256),
    diff: asString(object.diff),
    risk_level: asString(object.risk_level),
    requires_explicit_confirmation: asBool(object.requires_explicit_confirmation),
    restart_required: asBool(object.restart_required),
    allow_active: asBool(object.allow_active),
  };
}

export function normalizeOpticalConfigApply(raw: unknown): OpticalConfigApplyResult {
  const object = asObject(raw) ?? {};
  return {
    ok: Boolean(object.ok),
    source: asString(object.source) ?? "unavailable",
    error: asString(object.error),
    summary: asString(object.summary),
    transaction_id: asString(object.transaction_id),
    rollback_id: asString(object.rollback_id),
    config_applied: asBool(object.config_applied),
    config_restored: asBool(object.config_restored),
    runtime_verified: asBool(object.runtime_verified),
    restarted: asBool(object.restarted),
    restart_summary: asString(object.restart_summary),
    allow_active: asBool(object.allow_active),
  };
}

export async function loadOpticalConfigEditor(): Promise<OpticalConfigEditorSnapshot> {
  const raw = await invokeNative<unknown>("get_optical_config_editor");
  return normalizeOpticalConfigEditor(raw);
}

export async function previewOpticalConfigChange(draft: OpticalConfigDraft): Promise<OpticalConfigReview> {
  const raw = await invokeNative<unknown>("preview_optical_config_change", { request: { draft } });
  return normalizeOpticalConfigReview(raw);
}

export async function applyOpticalConfigChange(
  draft: OpticalConfigDraft,
  reviewed: { before: string; after: string },
  restartService = true,
): Promise<OpticalConfigApplyResult> {
  const raw = await invokeNative<unknown>("apply_optical_config_change", {
    request: {
      draft,
      reviewed_before_sha256: reviewed.before,
      reviewed_after_sha256: reviewed.after,
      restart_service: restartService,
    },
  });
  return normalizeOpticalConfigApply(raw);
}

export async function startOpticalService(): Promise<NativeActionResult> {
  return invokeNative<NativeActionResult>("start_dtoken_service");
}

export async function restartOpticalService(): Promise<NativeActionResult> {
  return invokeNative<NativeActionResult>("restart_dtoken_service");
}

export async function restoreOpticalConfigBackup(rollbackId: string): Promise<OpticalConfigApplyResult> {
  const raw = await invokeNative<unknown>("rollback_optical_config_change", {
    request: { rollback_id: rollbackId },
  });
  return normalizeOpticalConfigApply(raw);
}
