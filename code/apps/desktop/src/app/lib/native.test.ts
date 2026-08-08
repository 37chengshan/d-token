import { describe, expect, it } from "vitest";
import { spectaCommands } from "./native";

describe("spectaCommands (fail-closed)", () => {
  it("throws native_unavailable in non-Tauri runtime", async () => {
    await expect(spectaCommands.getSpectaIpcStatus()).rejects.toThrow("native_unavailable");
  });

  it("throws native_unavailable for any command name in non-Tauri runtime", async () => {
    await expect(spectaCommands.getDesktopSummary()).rejects.toThrow("native_unavailable");
    await expect(spectaCommands.runDoctor()).rejects.toThrow("native_unavailable");
  });

  it("returns a function for any property access before resolving", async () => {
    const fn = spectaCommands.getSpectaIpcStatus;
    expect(typeof fn).toBe("function");
    await expect(fn()).rejects.toThrow("native_unavailable");
  });
});