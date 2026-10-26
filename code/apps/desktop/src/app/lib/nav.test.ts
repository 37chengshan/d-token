import { describe, expect, it } from "vitest";
import { NAV, SETTINGS_NAV, migratePageId, type PageId } from "./nav";

describe("Five-page navigation contract", () => {
  it("defines the four workbench destinations plus settings", () => {
    const expectedPrimary: PageId[] = [
      "configuration",
      "advanced-configuration",
      "analysis-logs",
      "diagnostics",
    ];
    expect(NAV.map((item) => item.id)).toEqual(expectedPrimary);
    expect(SETTINGS_NAV.id).toBe("settings");
  });

  it("migrates legacy session and deep-link targets to the owning page", () => {
    expect(migratePageId("overview")).toBe("configuration");
    expect(migratePageId("install")).toBe("configuration");
    expect(migratePageId("codebase-memory")).toBe("configuration");
    expect(migratePageId("visual")).toBe("configuration");
    expect(migratePageId("advanced")).toBe("advanced-configuration");
    expect(migratePageId("analytics")).toBe("analysis-logs");
    expect(migratePageId("changes")).toBe("analysis-logs");
    expect(migratePageId("logs")).toBe("analysis-logs");
    expect(migratePageId("doctor")).toBe("diagnostics");
    expect(migratePageId("compatibility")).toBe("diagnostics");
    expect(migratePageId("unknown-page")).toBe("configuration");
    expect(migratePageId("__proto__")).toBe("configuration");
    expect(migratePageId("constructor")).toBe("configuration");
    expect(migratePageId(null)).toBe("configuration");
  });
});
