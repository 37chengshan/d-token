/**
 * Closed protocol adapter for the desktop UI.
 *
 * Rust/runtime payloads use the serde wire id `open_ai`; older aggregates and
 * a few legacy callers use `openai`. UI code must consume the canonical id or
 * localized label and must never send a label back as a protocol value.
 */

import { translateCopy } from "./i18n";

export type ProtocolId = "openai" | "anthropic";
export type ProtocolWire = "open_ai" | "anthropic";
export type ProtocolLanguage = "zh" | "en";

export type ParsedProtocol = {
  id: ProtocolId | null;
  wire: ProtocolWire | null;
  label: string | null;
  known: boolean;
};

const PROTOCOLS: Readonly<Record<ProtocolId, { wire: ProtocolWire; labels: Record<ProtocolLanguage, string> }>> = {
  openai: { wire: "open_ai", labels: { zh: "OpenAI", en: "OpenAI" } },
  anthropic: { wire: "anthropic", labels: { zh: "Anthropic", en: "Anthropic" } },
};

/** The external capability id is intentionally distinct from protocol ids and route kinds. */
export const CC_SWITCH_CAPABILITY_ID = "cc-switch" as const;

function normalized(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const text = value.trim().toLowerCase();
  return text.length > 0 ? text : null;
}

export function parseProtocol(value: unknown, lang: ProtocolLanguage = "en"): ParsedProtocol {
  const text = normalized(value);
  if (!text) return { id: null, wire: null, label: null, known: false };

  const id: ProtocolId | null = text === "open_ai" || text === "openai"
    ? "openai"
    : text === "anthropic"
      ? "anthropic"
      : null;
  if (!id) return { id: null, wire: null, label: translateCopy("protocol.unknown", lang), known: false };

  return {
    id,
    wire: PROTOCOLS[id].wire,
    label: PROTOCOLS[id].labels[lang],
    known: true,
  };
}

/** Canonical id for comparison and UI state; unknown values fail closed. */
export function protocolId(value: unknown): ProtocolId | null {
  return parseProtocol(value).id;
}

/** Runtime/config wire id. `openai` and `open_ai` both serialize as `open_ai`. */
export function toWireProtocol(value: unknown): ProtocolWire | null {
  const id = protocolId(value);
  return id ? PROTOCOLS[id].wire : null;
}

/** Alias kept explicit for call sites that describe serialization. */
export const protocolToWire = toWireProtocol;

/** Parse a runtime wire value to its canonical UI/comparison id. */
export function fromWireProtocol(value: unknown): ProtocolId | null {
  return protocolId(value);
}

/** Alias kept explicit for call sites that describe deserialization. */
export const protocolFromWire = fromWireProtocol;

/** Re-serialize any accepted protocol form without ever serializing a UI label. */
export function roundTripProtocol(value: unknown): ProtocolWire | null {
  return toWireProtocol(value);
}

/** Localized display label. Unknown non-empty values are intentionally visible as Unknown/未知. */
export function protocolLabel(value: unknown, lang: ProtocolLanguage = "en"): string | null {
  return parseProtocol(value, lang).label;
}

export const protocolDisplayName = protocolLabel;

export function isProtocol(value: unknown, id: ProtocolId): boolean {
  return protocolId(value) === id;
}

/** Capability ids are not protocol ids; do not normalize `cc_switch` into this value. */
export function isCcSwitchCapabilityId(value: unknown): value is typeof CC_SWITCH_CAPABILITY_ID {
  return value === CC_SWITCH_CAPABILITY_ID;
}
