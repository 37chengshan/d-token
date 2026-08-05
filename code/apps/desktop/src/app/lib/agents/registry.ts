/**
 * Shared presentation-only shape for legacy cc-switch comparison helpers.
 *
 * Agent identities and capabilities are intentionally not registered here.
 * The Rust discovery registry is the sole source of detected Agent rows,
 * transport support, configuration targets, and Configure/Clear availability.
 */
export type UpstreamDescriptor = {
  source: "official" | "cc-switch" | "custom" | "local-provider" | "unknown";
  protocol: "openai-compatible" | "anthropic-compatible" | "local" | "unknown";
  baseUrl?: string;
  port?: number;
  profileName?: string;
  providerName?: string;
  redactedCredentialRef?: string;
  confidence: number;
};
