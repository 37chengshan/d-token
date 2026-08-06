/**
 * Protocol presentation adapter for the Analytics surface.
 *
 * Rust serializes Protocol as snake_case (`open_ai`, `anthropic`), while
 * older aggregate payloads may contain `openai`. Unknown values fail closed:
 * their raw text is never presented and non-empty unknown input becomes the
 * canonical `unknown` label.
 */

export type ProtocolId = "openai" | "anthropic";

export type ParsedProtocol = {
  id: ProtocolId | null;
  label: string | null;
  known: boolean;
};

const PROTOCOL_ID_BY_WIRE: Record<string, ProtocolId> = {
  open_ai: "openai",
  openai: "openai",
  anthropic: "anthropic",
};

export function parseProtocol(value: unknown, _lang: "zh" | "en" = "en"): ParsedProtocol {
  if (typeof value !== "string") return { id: null, label: null, known: false };
  const wire = value.trim().toLowerCase();
  if (!wire) return { id: null, label: null, known: false };
  const id = PROTOCOL_ID_BY_WIRE[wire];
  if (id) return { id, label: id, known: true };
  return { id: null, label: "unknown", known: false };
}

export function protocolLabel(value: unknown, lang: "zh" | "en" = "en"): string | null {
  return parseProtocol(value, lang).label;
}

export function protocolId(value: unknown): ProtocolId | null {
  return parseProtocol(value).id;
}

export function isProtocol(value: unknown, id: ProtocolId): boolean {
  return parseProtocol(value).id === id;
}
