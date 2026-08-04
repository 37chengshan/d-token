export type Protocol = "open_ai" | "anthropic";

export type CompressionPolicy =
  | "off"
  | "safe"
  | "balanced"
  | "aggressive"
  | "max";

export type OpticalEndpoint =
  | "anthropic_messages"
  | "open_ai_chat_completions"
  | "open_ai_responses"
  | "unsupported";

export type CompressionDecisionState = "resolved" | "unavailable";
export type CompressionArtifactState = "transformed" | "passthrough";
export type CompressionExecutionState = "completed" | "passthrough";

export interface CompressionDecisionRequest {
  route_id: string;
  input_protocol: Protocol;
  endpoint: OpticalEndpoint;
  requested_policy?: CompressionPolicy;
  body: Record<string, unknown>;
}

export interface CompressionArtifactIdentity {
  state: CompressionArtifactState;
  identity: string;
  transforms_applied: string[];
}

export interface CompressionDecisionEvidence {
  route_identity: string;
  provider_identity: string;
  model_allowlist: string;
  endpoint_identity: string;
  request_body_persisted: boolean;
  credentials_returned: boolean;
}

export interface CompressionDecision {
  contract_revision: string;
  source: string;
  state: CompressionDecisionState;
  reason_code: string;
  route_id: string;
  provider_id: string;
  model: string;
  input_protocol: Protocol;
  output_protocol: Protocol;
  endpoint: OpticalEndpoint;
  compression_policy: CompressionPolicy;
  artifact: CompressionArtifactIdentity;
  original_tokens: number;
  compressed_tokens: number;
  tokens_saved: number;
  evidence: CompressionDecisionEvidence;
}

export interface CompressionExecution {
  contract_revision: string;
  source: "dtoken-owned-runtime";
  state: CompressionExecutionState;
  body: Record<string, unknown>;
  original_tokens: number;
  compressed_tokens: number;
  tokens_saved: number;
  savings_percent: number;
  elapsed_us: number;
  transforms_applied: string[];
  fallback_reason_code?: string;
  evidence: CompressionDecisionEvidence;
  decision: CompressionDecision;
  note: string;
}

export interface RuntimeRequestTotals {
  total: number;
  failed: number;
}

export interface RuntimeTokenTotals {
  input: number;
  input_after: number;
  output: number;
  saved: number;
  signed_net: number;
  positive_saved: number;
  penalty: number;
  savings_percent: number;
}

export interface RuntimeStatsSummary {
  status: "empty_runtime" | "runtime";
  source: "dtoken-owned-runtime";
  runtime_kind: "headroom-derived";
  external_headroom_dependency: false;
  compressions: number;
  tokens_saved: number;
  signed_net_tokens: number;
  positive_saved_tokens: number;
  penalty_tokens: number;
  savings_percent: number;
  avg_latency_ms: number | null;
  avg_latency_semantics: "compression_only";
  requests: RuntimeRequestTotals;
  tokens: RuntimeTokenTotals;
}

export interface RuntimeMetricsSummary {
  available: true;
  source: "dtoken-owned-runtime";
  runtime_kind: "headroom-derived";
  external_headroom_dependency: false;
  compression_performance: {
    source: "request_logs";
    total_requests: number;
    transforms: Array<Record<string, unknown>>;
    signed_net_tokens: number;
    positive_saved_tokens: number;
    penalty_tokens: number;
  };
  route_performance: {
    source: "request_logs";
    routes: Array<Record<string, unknown>>;
    providers: Array<Record<string, unknown>>;
    policies: Array<Record<string, unknown>>;
    latency_breakdown: Record<string, unknown>;
    route_overhead: Record<string, unknown>;
  };
  prometheus_text: string;
}

export interface CcrMetadataRecord {
  recovery_ref?: string;
  request_id?: string;
  state?: string;
  tokens_saved?: number;
  raw_available?: boolean;
  raw_returned?: boolean;
}

export interface CcrSummary {
  available: boolean;
  source: "dtoken-owned-runtime";
  runtime_kind: "headroom-derived";
  external_headroom_dependency: false;
  entries: number;
  private_entries: number;
  bytes: number;
  retrievals: number;
  tokens_saved: number;
  raw_available: boolean;
  private_store_configured: boolean;
  raw_access: "in-process-authorized-only" | "disabled";
  recent_refs: CcrMetadataRecord[];
  note: string;
}

export interface CcrMetadataLookup {
  source: "dtoken-owned-runtime";
  recovery_ref: string;
  raw_available: boolean;
  raw_returned?: false;
  retrieval_boundary?: "in-process-authorized-only" | "disabled";
  metadata?: CcrMetadataRecord;
  error?: {
    type: "ccr_metadata_not_found";
    message: string;
  };
}

export interface MemorySummary {
  source: "dtoken-owned-runtime";
  runtime_kind: "headroom-derived";
  state: "scope_required" | "unavailable" | "invalid_scope" | "ready";
  entries: number;
  raw_available: false;
  recent: Array<Record<string, unknown>>;
  reason_code?: string;
  note?: string;
}

export type OpticalMode =
  | "off"
  | "shadow_preview"
  | "tool_results_only"
  | "history_only"
  | "full_experimental";

export interface OpticalStatusSummary {
  schema_version: number;
  enabled: boolean;
  configured_mode: OpticalMode;
  effective_mode: OpticalMode;
  default_mode: OpticalMode;
  allow_active: boolean;
  active_reachable: boolean;
  active_execution_observed: boolean;
  public_summary_only: boolean;
  fail_closed: true;
  renderer_ready: boolean;
  patcher_ready: boolean;
  post_validator_ready: boolean;
  signed_accounting_ready: boolean;
  representation_plan_ready: boolean;
  responses_shadow_inventory_ready: boolean;
  dynamic_page_accounting_ready: boolean;
  active_direct_routes: number;
  verified_attested_capabilities: number;
  rejected_capabilities: number;
  config_issue_count: number;
}

export interface OpticalCapabilitiesSummary {
  shadow_preview_supported: boolean;
  active_transform_enabled: boolean;
  max_mode: OpticalMode;
  supported_endpoints: OpticalEndpoint[];
  unknown_model_status: string;
  unsupported_path_status: string;
  public_summary_only: boolean;
  fail_closed: true;
  direct_anthropic_upstream_supported: boolean;
  streaming_passthrough_supported: boolean;
  deterministic_png_renderer: boolean;
  artifact_post_validation: boolean;
  signed_multimodal_accounting: boolean;
  capability_attestation_required: boolean;
}

export interface McpToolAvailability {
  name: string;
  reason: string;
}

export interface McpStatusSummary {
  source: "dtoken-owned-runtime";
  state: string;
  configured: boolean;
  transport: string | null;
  http_transport: string;
  tools: string[];
  unavailable_tools: McpToolAvailability[];
  raw_available: false;
  reason: string;
}

export interface CompressionDecisionErrorPayload {
  error?: {
    type?: string;
    code?: string;
    message?: string;
  };
}

export interface DtokenCompressionClientOptions {
  baseUrl: string;
  fetch?: typeof fetch;
  headers?: HeadersInit;
}

export class DtokenCompressionDecisionError extends Error {
  readonly status: number;
  readonly code: string;
  readonly type?: string;

  constructor(status: number, payload: CompressionDecisionErrorPayload) {
    const code = payload.error?.code ?? "unknown_error";
    super(payload.error?.message ?? `d-token compression decision request failed (${status})`);
    this.name = "DtokenCompressionDecisionError";
    this.status = status;
    this.code = code;
    this.type = payload.error?.type;
  }
}

export class DtokenReadError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, payload: CompressionDecisionErrorPayload) {
    const code = payload.error?.code ?? payload.error?.type ?? "read_unavailable";
    super(payload.error?.message ?? `d-token read request failed (${status})`);
    this.name = "DtokenReadError";
    this.status = status;
    this.code = code;
  }
}

export class DtokenCompressionClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;
  private readonly headers?: HeadersInit;

  constructor(options: DtokenCompressionClientOptions) {
    this.baseUrl = normalizeBaseUrl(options.baseUrl);
    this.fetchImpl = options.fetch ?? fetch;
    this.headers = options.headers;
  }

  async compressionDecision(
    request: CompressionDecisionRequest,
  ): Promise<CompressionDecision> {
    const response = await this.fetchImpl(
      `${this.baseUrl}/v1/compression/decision`,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          ...this.headers,
        },
        body: JSON.stringify(request),
      },
    );

    const payload = (await response.json()) as
      | CompressionDecision
      | CompressionDecisionErrorPayload;

    if (!response.ok) {
      throw new DtokenCompressionDecisionError(
        response.status,
        payload as CompressionDecisionErrorPayload,
      );
    }

    return payload as CompressionDecision;
  }

  /**
   * Produces a validated, route-bound candidate body without forwarding it.
   * The caller may send `body` to the selected gateway/provider after checking
   * the returned decision evidence.
   */
  async compress(
    request: CompressionDecisionRequest,
  ): Promise<CompressionExecution> {
    const response = await this.fetchImpl(`${this.baseUrl}/v1/compress`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...this.headers,
      },
      body: JSON.stringify(request),
    });

    const payload = (await response.json()) as
      | CompressionExecution
      | CompressionDecisionErrorPayload;

    if (!response.ok) {
      throw new DtokenCompressionDecisionError(
        response.status,
        payload as CompressionDecisionErrorPayload,
      );
    }

    return payload as CompressionExecution;
  }

  async runtimeStats(): Promise<RuntimeStatsSummary> {
    return this.readJson<RuntimeStatsSummary>("/stats");
  }

  async runtimeMetricsSummary(): Promise<RuntimeMetricsSummary> {
    const payload = await this.readJson<{ metrics: RuntimeMetricsSummary }>(
      "/headroom/stats",
    );
    return payload.metrics;
  }

  async ccrSummary(): Promise<CcrSummary> {
    return this.readJson<CcrSummary>("/ccr/summary");
  }

  /** Reads redacted CCR metadata only. This SDK never retrieves an original body. */
  async ccrMetadata(recoveryRef: string): Promise<CcrMetadataLookup> {
    return this.readJson<CcrMetadataLookup>(
      `/ccr/retrieve/${encodeURIComponent(recoveryRef)}`,
    );
  }

  /** The loopback HTTP surface intentionally cannot establish a memory scope. */
  async memorySummary(): Promise<MemorySummary> {
    return this.readJson<MemorySummary>("/memory/summary");
  }

  async opticalStatus(): Promise<OpticalStatusSummary> {
    return this.readJson<OpticalStatusSummary>("/compression/optical/status");
  }

  async opticalCapabilities(): Promise<OpticalCapabilitiesSummary> {
    return this.readJson<OpticalCapabilitiesSummary>(
      "/compression/optical/capabilities",
    );
  }

  async mcpStatus(): Promise<McpStatusSummary> {
    return this.readJson<McpStatusSummary>("/headroom/mcp");
  }

  /**
   * Calls the Streamable-HTTP MCP JSON transport (`POST /mcp`).
   *
   * The endpoint is stateless and capability-free, exactly like the stdio
   * transport: only the tools the runtime exposes without a managed
   * capability (`headroom_compress`, `headroom_stats`) are reachable.
   * `notifications` (requests without an `id`) resolve to `null`.  Errors are
   * surfaced as JSON-RPC errors with stable codes.
   */
  async mcpRequest<T = Record<string, unknown>>(
    method: string,
    params?: Record<string, unknown>,
    id?: number | string | null,
  ): Promise<T | null> {
    const response = await this.fetchImpl(`${this.baseUrl}/mcp`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json",
        ...this.headers,
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: id ?? 1,
        method,
        ...(params === undefined ? {} : { params }),
      }),
    });
    if (response.status === 202) {
      return null;
    }
    const payload = (await response.json()) as
      | { result?: T; error?: { code?: number; message?: string } }
      | CompressionDecisionErrorPayload;
    if (!response.ok) {
      const error = (payload as { error?: { code?: number; message?: string } }).error;
      throw new DtokenReadError(response.status, {
        error: {
          code: String(error?.code ?? "mcp_request_failed"),
          message: error?.message ?? `MCP request failed (${response.status})`,
        },
      });
    }
    const result = (payload as { result?: T }).result;
    if (result === undefined) {
      throw new DtokenReadError(response.status, {
        error: { code: "mcp_result_unavailable", message: "MCP request returned no result" },
      });
    }
    return result as T;
  }

  private async readJson<T>(path: string): Promise<T> {
    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method: "GET",
      headers: this.headers,
    });
    const payload = (await response.json()) as T | CompressionDecisionErrorPayload;
    if (!response.ok) {
      throw new DtokenReadError(
        response.status,
        payload as CompressionDecisionErrorPayload,
      );
    }
    return payload as T;
  }
}

export async function compressionDecision(
  options: DtokenCompressionClientOptions,
  request: CompressionDecisionRequest,
): Promise<CompressionDecision> {
  return new DtokenCompressionClient(options).compressionDecision(request);
}

export async function compress(
  options: DtokenCompressionClientOptions,
  request: CompressionDecisionRequest,
): Promise<CompressionExecution> {
  return new DtokenCompressionClient(options).compress(request);
}

function normalizeBaseUrl(baseUrl: string): string {
  return baseUrl.replace(/\/+$/, "");
}
