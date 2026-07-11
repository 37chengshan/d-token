import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));
const runtime = vi.hoisted(() => ({ refresh: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
  isTauriRuntime: () => true,
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => ({
    payload: null,
    status: "browser-preview",
    updatedAt: null,
    error: "",
    refresh: runtime.refresh,
  }),
}));

import {
  Advanced,
  isUsableRouteReview,
  routeApplyFailureMessage,
  routeApplyRequest,
} from "./Advanced";

const beforeSha = "a".repeat(64);
const afterSha = "b".repeat(64);

const review = {
  ok: true,
  source: "dtoken-desktop-control-plane",
  mode: "route-apply-review" as const,
  route_id: "route-reviewed",
  target: "/Users/test/Library/Application Support/d-token/active-config.json",
  before_sha256: beforeSha,
  after_sha256: afterSha,
  diff: "--- before\n+++ after\n@@ -1 +1 @@\n-old\n+new",
  risk_level: "medium",
  requires_explicit_confirmation: true,
  runtime_required: false,
  privacy: "read-only review",
};

function installNativeMock(applyResult = {
  ok: true,
  summary: "Reviewed route draft applied and verified with a durable rollback point",
  logs: [],
  tools: [],
}, reviewResult: unknown = review) {
  native.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_active_route_config_status") return { state: "missing" };
    if (command === "list_route_profile_drafts") {
      return {
        drafts: [{
          savedAt: "2026-07-12T00:00:00Z",
          config: {
            providers: [{ id: "provider-reviewed", name: "Reviewed provider", base_url: "https://provider.example" }],
            routes: [{ id: "route-reviewed", name: "Reviewed route", local_port: 19001 }],
          },
        }],
      };
    }
    if (command === "get_app_settings") return { state: "missing" };
    if (command === "preview_route_profile_draft_apply") return reviewResult;
    if (command === "apply_route_profile_draft") return applyResult;
    return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
  });
}

beforeEach(() => {
  native.invokeNative.mockReset();
  runtime.refresh.mockReset();
});

afterEach(cleanup);

describe("Advanced reviewed route apply", () => {
  it("requires a complete control-plane review and binds apply to both SHA-256 values", async () => {
    const user = userEvent.setup();
    installNativeMock();
    render(<Advanced />);

    await user.click(screen.getByRole("tab", { name: "Providers & Routes" }));
    await user.click(await screen.findByRole("button", { name: "审阅并应用" }));

    const dialog = await screen.findByRole("alertdialog", { name: "审阅并应用路由" });
    expect(within(dialog).getByText(review.target)).toBeVisible();
    expect(within(dialog).getByText(/--- before/)).toBeVisible();
    expect(within(dialog).getByText(beforeSha)).toBeVisible();
    expect(within(dialog).getByText(afterSha)).toBeVisible();
    expect(native.invokeNative).toHaveBeenCalledWith("preview_route_profile_draft_apply", {
      request: { route_id: "route-reviewed" },
    });

    await user.click(within(dialog).getByRole("button", { name: "确认应用" }));

    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("apply_route_profile_draft", {
      request: {
        route_id: "route-reviewed",
        reviewed_before_sha256: beforeSha,
        reviewed_after_sha256: afterSha,
      },
    }));
    expect(await screen.findByText("路由草稿已应用并完成写后验证；事务已创建持久回滚点。")).toBeVisible();
  });

  it("closes a stale review and explains that the current diff must be reviewed again", async () => {
    const user = userEvent.setup();
    installNativeMock({
      ok: false,
      summary: "Route review is stale; review the current diff again",
      logs: [],
      tools: [],
    });
    render(<Advanced />);

    await user.click(screen.getByRole("tab", { name: "Providers & Routes" }));
    await user.click(await screen.findByRole("button", { name: "审阅并应用" }));
    const dialog = await screen.findByRole("alertdialog", { name: "审阅并应用路由" });
    await user.click(within(dialog).getByRole("button", { name: "确认应用" }));

    expect(await screen.findByText("应用已阻止：草稿或活动配置在审阅后发生变化，请重新审阅当前差异。")).toBeVisible();
    await waitFor(() => expect(screen.queryByRole("alertdialog", { name: "审阅并应用路由" })).not.toBeInTheDocument());
  });

  it("does not expose confirmation or call apply when preview evidence is incomplete", async () => {
    const user = userEvent.setup();
    installNativeMock(undefined, {
      ok: false,
      source: "dtoken-desktop-control-plane",
      mode: "route-apply-review",
      error: "Route draft not found",
      requires_explicit_confirmation: true,
      runtime_required: false,
    });
    render(<Advanced />);

    await user.click(screen.getByRole("tab", { name: "Providers & Routes" }));
    await user.click(await screen.findByRole("button", { name: "审阅并应用" }));

    expect(await screen.findByText("无法审阅：路由草稿不存在或已被移除。")).toBeVisible();
    expect(screen.queryByRole("alertdialog", { name: "审阅并应用路由" })).not.toBeInTheDocument();
    expect(native.invokeNative.mock.calls.some(([command]) => command === "apply_route_profile_draft")).toBe(false);
  });

  it("rejects missing hashes and never creates a partial apply payload", () => {
    expect(isUsableRouteReview({ ...review, after_sha256: "" }, "route-reviewed")).toBe(false);
    expect(isUsableRouteReview(review, "another-route")).toBe(false);
    expect(routeApplyRequest(review)).toEqual({
      route_id: "route-reviewed",
      reviewed_before_sha256: beforeSha,
      reviewed_after_sha256: afterSha,
    });
    expect(routeApplyFailureMessage("Review the route diff before applying it", "zh"))
      .toBe("应用已阻止：请先审阅当前路由差异。");
    expect(routeApplyFailureMessage("Route review is stale; review the current diff again", "en"))
      .toBe("Apply was blocked because the draft or active config changed after review. Review the current diff again.");
  });
});
