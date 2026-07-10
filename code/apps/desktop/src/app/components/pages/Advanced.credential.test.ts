import { describe, expect, it } from "vitest";

import {
  credentialReferenceForProvider,
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
});
