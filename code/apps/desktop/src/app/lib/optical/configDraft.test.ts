import { describe, expect, it } from "vitest";
import {
  buildOpticalConfigPatch,
  clampOpticalMode,
  defaultOpticalDraft,
  normalizeOpticalDraft,
  opticalDraftsEqual,
  summarizeOpticalDraft,
} from "./configDraft";

describe("optical config draft", () => {
  const routes = [
    { route_id: "route-a", label: "A" },
    { route_id: "route-b", label: "B" },
  ];

  it("clamps active-leaning modes to shadow_preview", () => {
    expect(clampOpticalMode("tool_results_only")).toBe("shadow_preview");
    expect(clampOpticalMode("full_experimental")).toBe("shadow_preview");
    expect(clampOpticalMode("off")).toBe("off");
    expect(clampOpticalMode("shadow_preview")).toBe("shadow_preview");
  });

  it("rejects unknown route ids", () => {
    const result = normalizeOpticalDraft(
      {
        global: { enabled: true, default_mode: "shadow_preview" },
        routes: [
          {
            route_id: "missing",
            enabled: true,
            mode: "shadow_preview",
            allow_tool_result: true,
            allow_history: false,
            confirmed_experimental_lossy: false,
          },
        ],
      },
      routes,
    );
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toMatch(/unknown route_id/);
  });

  it("builds a patch that always forces allow_active false", () => {
    const draft = defaultOpticalDraft(routes);
    draft.global.enabled = true;
    draft.routes[0].enabled = true;
    draft.routes[0].mode = "shadow_preview";
    draft.routes[0].allow_tool_result = true;
    const patch = buildOpticalConfigPatch(draft);
    const global = patch.global_policy as Record<string, unknown>;
    expect(global.allow_active).toBe(false);
    expect(global.public_summary_only).toBe(true);
    expect(global.enabled).toBe(true);
    const policies = patch.route_policies as Array<{ policy: Record<string, unknown> }>;
    expect(policies[0].policy.allow_schema_annotations).toBe(false);
    expect(policies[0].policy.allow_system_static_slab).toBe(false);
  });

  it("detects dirty drafts and summarizes in both languages", () => {
    const a = defaultOpticalDraft(routes);
    const b = defaultOpticalDraft(routes);
    expect(opticalDraftsEqual(a, b)).toBe(true);
    b.global.enabled = true;
    expect(opticalDraftsEqual(a, b)).toBe(false);
    expect(summarizeOpticalDraft(b, "zh")).toMatch(/Active 改写：强制关闭/);
    expect(summarizeOpticalDraft(b, "en")).toMatch(/Active transform: forced off/);
  });
});
