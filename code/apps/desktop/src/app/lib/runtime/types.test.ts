import { describe, expect, it } from "vitest";
import { DesktopDataSchema } from "./types";

describe("DesktopDataSchema", () => {
  it("accepts d-token desktop data with extension fields", () => {
    const result = DesktopDataSchema.safeParse({
      desktop_revision: "runtime-revision-1",
      desktop_data_source: "dtoken-local-service",
      stats: { requests: { total: 4 } },
    });

    expect(result.success).toBe(true);
    if (result.success) expect(result.data.stats).toEqual({ requests: { total: 4 } });
  });

  it("rejects unversioned or blank runtime payloads", () => {
    expect(DesktopDataSchema.safeParse({ source: "dtoken-owned-runtime" }).success).toBe(false);
    expect(DesktopDataSchema.safeParse({ desktop_revision: "  " }).success).toBe(false);
  });
});
