import { describe, expect, it } from "vitest";
import {
  CC_SWITCH_CAPABILITY_ID,
  fromWireProtocol,
  isCcSwitchCapabilityId,
  parseProtocol,
  protocolId,
  protocolLabel,
  roundTripProtocol,
  toWireProtocol,
} from "./protocol";

describe("closed protocol adapter", () => {
  it.each(["open_ai", "openai"])("accepts %s and preserves the OpenAI wire id", (value) => {
    expect(parseProtocol(value)).toEqual({ id: "openai", wire: "open_ai", label: "OpenAI", known: true });
    expect(protocolId(value)).toBe("openai");
    expect(toWireProtocol(value)).toBe("open_ai");
  });

  it("accepts the Anthropic wire id", () => {
    expect(parseProtocol("anthropic")).toEqual({ id: "anthropic", wire: "anthropic", label: "Anthropic", known: true });
    expect(fromWireProtocol("anthropic")).toBe("anthropic");
    expect(roundTripProtocol("anthropic")).toBe("anthropic");
  });

  it("fails closed for unknown and missing values", () => {
    expect(parseProtocol("google")).toEqual({ id: null, wire: null, label: "Unknown", known: false });
    expect(parseProtocol("google", "zh").label).toBe("未知");
    expect(parseProtocol(undefined, "zh")).toEqual({ id: null, wire: null, label: null, known: false });
    expect(toWireProtocol("google")).toBeNull();
  });

  it("keeps labels localized while canonical ids and wire payloads stay stable", () => {
    expect(protocolLabel("open_ai", "zh")).toBe("OpenAI");
    expect(protocolLabel("anthropic", "zh")).toBe("Anthropic");
    expect(protocolLabel("open_ai", "en")).toBe("OpenAI");
    expect(roundTripProtocol("OpenAI")).toBe("open_ai");
    expect(roundTripProtocol(protocolLabel("open_ai", "zh"))).toBe("open_ai");
  });

  it("labels Responses discovery without silently rewriting its wire protocol", () => {
    expect(protocolLabel("responses", "zh")).toBe("OpenAI Responses");
    expect(protocolLabel("open_ai_responses", "en")).toBe("OpenAI Responses");
    expect(toWireProtocol("responses")).toBeNull();
  });

  it("keeps cc-switch capability ids separate from protocol ids", () => {
    expect(CC_SWITCH_CAPABILITY_ID).toBe("cc-switch");
    expect(isCcSwitchCapabilityId("cc-switch")).toBe(true);
    expect(isCcSwitchCapabilityId("cc_switch")).toBe(false);
    expect(protocolId(CC_SWITCH_CAPABILITY_ID)).toBeNull();
    expect(toWireProtocol(CC_SWITCH_CAPABILITY_ID)).toBeNull();
  });
});
