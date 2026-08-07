import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../native", () => ({
  invokeNative: (...args: unknown[]) => mocks.invokeNative(...args),
}));

import { normalizeOpticalPreview, previewOpticalPlan } from "./client";

const PREVIEW_RESPONSE = {
  ok: true,
  source: "runtime",
  error: "",
  data: {
    plan_id: "plan-1",
    mode: "shadow_preview",
    final_status: "eligible",
    gates: [{ name: "UnsupportedPath", passed: false, reason: "path is outside project" }],
    warnings: ["sample reduced"],
    counters: { candidate_blocks: 2, preserved_blocks: 1, skipped_blocks: 1, estimated_images: 0 },
    economics: { net_saved_tokens: 0 },
  },
};

describe("previewOpticalPlan request contract", () => {
  it("passes an optional route_id through while provider/model stay null", async () => {
    mocks.invokeNative.mockResolvedValue(PREVIEW_RESPONSE);
    await previewOpticalPlan({ messages: [] }, { route_id: "route-a" });
    expect(mocks.invokeNative).toHaveBeenCalledWith("preview_optical_plan", {
      request: {
        endpoint_path: "/v1/messages",
        protocol: "anthropic",
        provider_id: null,
        route_id: "route-a",
        model: null,
        body: { messages: [] },
      },
    });
  });

  it("defaults route_id to null when no route is selected", async () => {
    mocks.invokeNative.mockResolvedValue(PREVIEW_RESPONSE);
    await previewOpticalPlan({ messages: [] });
    const call = mocks.invokeNative.mock.calls[0];
    const request = (call[1] as { request: Record<string, unknown> }).request;
    expect(request.provider_id).toBeNull();
    expect(request.route_id).toBeNull();
    expect(request.model).toBeNull();
  });

  it("never adds credential, body-echo, or sensitive fields to the request", async () => {
    mocks.invokeNative.mockResolvedValue(PREVIEW_RESPONSE);
    await previewOpticalPlan({ messages: [] }, { route_id: "route-a" });
    const call = mocks.invokeNative.mock.calls[0];
    const request = (call[1] as { request: Record<string, unknown> }).request;
    const serialized = JSON.stringify(request);
    for (const forbidden of ["api_key", "apiKey", "authorization", "token", "cookie", "base64", "image/png", "raw_prompt", "secret"]) {
      expect(serialized.toLowerCase()).not.toContain(forbidden);
    }
    expect(request.body).toEqual({ messages: [] });
  });
});

describe("normalizeOpticalPreview", () => {
  it("maps gates with name/passed/reason and warnings from the public summary", () => {
    const result = normalizeOpticalPreview(PREVIEW_RESPONSE);
    expect(result.ok).toBe(true);
    expect(result.data?.gates).toEqual([
      { name: "UnsupportedPath", passed: false, reason: "path is outside project" },
    ]);
    expect(result.data?.warnings).toEqual(["sample reduced"]);
    expect(result.data?.mode).toBe("shadow_preview");
  });

  it("keeps ok=false and an error when the preview fails", () => {
    const result = normalizeOpticalPreview({ ok: false, source: "unavailable", error: "boom", data: null });
    expect(result.ok).toBe(false);
    expect(result.error).toBe("boom");
    expect(result.data).toBeNull();
  });
});
