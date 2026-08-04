import { describe, expect, it } from "vitest";

import {
  credentialReferenceForProvider,
  isDtokenSelfLoop,
  validateCredentialInput,
} from "./Advanced";

describe("Advanced provider credentials", () => {
  it("uses only an opaque OS credential-store reference in route config", () => {
    expect(credentialReferenceForProvider("OpenCode Zen")).toBe(
      "os-credential://d-token/providers/opencode-zen",
    );
  });

  it("rejects empty and unreasonably large credential input", () => {
    expect(validateCredentialInput("  ")).toBe(false);
    expect(validateCredentialInput("unit-test-credential")).toBe(true);
    expect(validateCredentialInput("x".repeat(65_537))).toBe(false);
  });

  it("rejects an upstream that points back to the d-token listener", () => {
    expect(isDtokenSelfLoop("http://127.0.0.1:18001/v1", "18001")).toBe(true);
    expect(isDtokenSelfLoop("http://localhost:18001", "18001")).toBe(true);
    expect(isDtokenSelfLoop("https://api.example.com/v1", "18001")).toBe(false);
  });
});
