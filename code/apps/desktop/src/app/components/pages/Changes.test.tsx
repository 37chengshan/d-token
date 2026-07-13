import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

const beforeSha = "a".repeat(64);
const afterSha = "b".repeat(64);
const routeReview = {
  ok: true,
  source: "dtoken-desktop-control-plane",
  mode: "route-apply-review",
  route_id: "route-a",
  target: "D:/d-token/active-config/dtoken-config.json",
  before_sha256: beforeSha,
  after_sha256: afterSha,
  diff: "--- before\n+++ after\n@@ -1 +1 @@\n-old\n+new",
  risk_level: "medium",
  requires_explicit_confirmation: true,
  runtime_required: false,
};

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
}));

import { Changes } from "./Changes";

const desktopPayload = (verified = true) => ({
  source: "dtoken-owned-runtime",
  desktop_data_source: "dtoken-local-service",
  freshness: "fresh",
  complete: true,
  transaction_provenance: {
    source: "dtoken-owned-runtime",
    command: "get_desktop_data",
    schema_version: 1,
    generated_at: "2026-07-10T12:00:00Z",
  },
  routeDrafts: [
    {
      path: "D:/d-token/routing/route-a.json",
      config: {
        routes: [{ id: "route-a", local_port: 18001 }],
        providers: [{ id: "provider-a", base_url: "https://provider.example" }],
      },
      diff: [
        { type: "ctx", text: "[route.route-a]" },
        { type: "add", text: "+ local_port = 18001" },
      ],
    },
  ],
  changes: [],
  backupPoints: [
    {
      id: verified ? "backup-verified" : "backup-unverified",
      when: "2026-07-10 12:00",
      files: 1,
      ports: [18001],
      procs: 1,
      before: "warning",
      after: "healthy",
      verified,
    },
  ],
});

function useDesktopPayload(verified = true) {
  const payload = desktopPayload(verified);
  native.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_desktop_data") return payload;
    if (command === "preview_route_profile_draft_apply") return routeReview;
    return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
  });
}

beforeEach(() => {
  native.invokeNative.mockReset();
});

afterEach(cleanup);

describe("Changes action safety", () => {
  it("keeps browser preview empty instead of rendering fixture actions", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    render(<Changes />);

    expect(await screen.findByText("浏览器预览仅供查看，批准、拒绝和回退均不可执行。")).toBeVisible();
    expect(screen.getByText("当前没有待处理变更")).toBeVisible();
    expect(screen.queryByRole("button", { name: "批准" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "拒绝" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "备份与回滚" }));
    expect(await screen.findByText("当前没有可用回滚点")).toBeVisible();
    expect(screen.queryByRole("button", { name: "回滚到此处" })).not.toBeInTheDocument();
  });

  it("shows only ownership paths supplied by desktop evidence", async () => {
    const user = userEvent.setup();
    useDesktopPayload(true);
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    await user.click(screen.getByRole("tab", { name: "配置归属" }));
    expect(await screen.findByText("D:/d-token/routing/route-a.json")).toBeInTheDocument();
    expect(screen.queryByText("~/.claude/settings.json")).not.toBeInTheDocument();
    expect(screen.queryByText("~/.dtoken/events.db")).not.toBeInTheDocument();
  });

  it("does not render a route draft that native evidence marks as applied", async () => {
    const payload: any = desktopPayload(true);
    payload.routeDrafts[0] = {
      ...payload.routeDrafts[0],
      resolution: {
        state: "applied",
        after_sha256: afterSha,
        evidence: "verified-config-backup-after-hash",
      },
    };
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_data") return payload;
      return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
    });

    render(<Changes />);

    expect(await screen.findByText("当前没有待处理变更")).toBeVisible();
    expect(screen.getByText(/0 项待处理/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "批准" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "拒绝" })).not.toBeInTheDocument();
  });

  it.each([
    ["stale", { data_status: "stale" }, "桌面数据已过期，不能作为当前配置证据。批准、拒绝和回退均已锁定。"],
    ["partial", { partial: true }, "运行时数据不完整，批准和拒绝保持锁定；由桌面控制面验证过的本地回滚点仍可精确恢复。"],
  ])("keeps dangerous actions disabled for %s native payloads", async (_state, marker, message) => {
    const user = userEvent.setup();
    const payload = { ...desktopPayload(true), ...marker };
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_data") return payload;
      return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
    });
    render(<Changes />);

    expect(await screen.findByText(message)).toBeVisible();
    expect(screen.getByRole("button", { name: "批准" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "拒绝" })).toBeDisabled();

    await user.click(screen.getByRole("tab", { name: "备份与回滚" }));
    expect(await screen.findByRole("button", { name: "回滚到此处" })).toBeDisabled();
  });

  it.each([
    ["arbitrary payload", { routeDrafts: desktopPayload(true).routeDrafts, backupPoints: desktopPayload(true).backupPoints }],
    ["missing runtime identity", { ...desktopPayload(true), source: "desktop-fixture" }],
    ["missing freshness", { ...desktopPayload(true), freshness: undefined }],
    ["missing completeness", { ...desktopPayload(true), complete: undefined }],
    ["missing transaction provenance", { ...desktopPayload(true), transaction_provenance: undefined }],
    [
      "foreign transaction provenance",
      {
        ...desktopPayload(true),
        transaction_provenance: {
          source: "external-service",
          command: "get_desktop_data",
          schema_version: 1,
          generated_at: "2026-07-10T12:00:00Z",
        },
      },
    ],
  ])("fails closed for %s", async (_case, payload) => {
    const user = userEvent.setup();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_data") return payload;
      return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
    });
    render(<Changes />);

    expect(await screen.findByText("运行时数据不完整，批准和拒绝保持锁定；由桌面控制面验证过的本地回滚点仍可精确恢复。")).toBeVisible();
    expect(screen.getByRole("button", { name: "批准" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "拒绝" })).toBeDisabled();

    await user.click(screen.getByRole("tab", { name: "备份与回滚" }));
    expect(screen.getByRole("button", { name: "回滚到此处" })).toBeDisabled();
  });

  it("shows impact, backup and recovery information before approving a real change", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    useDesktopPayload(true);
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    fireEvent.click(screen.getByRole("button", { name: "批准" }));

    const dialog = await screen.findByRole("alertdialog", { name: "审阅并批准路由" });
    expect(within(dialog).getByText("影响范围")).toBeVisible();
    expect(within(dialog).getAllByText(routeReview.target).length).toBeGreaterThanOrEqual(1);
    expect(within(dialog).getByText("备份保障")).toBeVisible();
    expect(within(dialog).getByText(/backup-verified/)).toBeVisible();
    expect(within(dialog).getByText("恢复方式")).toBeVisible();
    expect(within(dialog).getByText(/回退/)).toBeVisible();
    expect(within(dialog).getByText(/--- before/)).toBeVisible();
    expect(within(dialog).getByText(beforeSha)).toBeVisible();
    expect(within(dialog).getByText(afterSha)).toBeVisible();
    expect(native.invokeNative).toHaveBeenCalledWith("preview_route_profile_draft_apply", {
      request: { route_id: "route-a" },
    });
    expect(native.invokeNative).not.toHaveBeenCalledWith("apply_route_profile_draft", expect.anything());

    fireEvent.click(within(dialog).getByRole("button", { name: "确认应用" }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("apply_route_profile_draft", {
        request: {
          route_id: "route-a",
          reviewed_before_sha256: beforeSha,
          reviewed_after_sha256: afterSha,
        },
      });
    });
    expect(errorSpy.mock.calls.flat().join(" ")).not.toContain("Function components cannot be given refs");
  });

  it("confirms a verified rollback point before restoring it", async () => {
    const user = userEvent.setup();
    useDesktopPayload(true);
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");
    await user.click(screen.getByRole("tab", { name: "备份与回滚" }));

    fireEvent.click(await screen.findByRole("button", { name: "回滚到此处" }));

    const dialog = await screen.findByRole("alertdialog", { name: "确认回退" });
    expect(within(dialog).getByText(/backup-verified/)).toBeVisible();
    expect(within(dialog).getByText(/1 个文件/)).toBeVisible();
    expect(native.invokeNative).not.toHaveBeenCalledWith("manage_backup", expect.anything());

    fireEvent.click(within(dialog).getByRole("button", { name: "确认回退" }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("manage_backup", {
        action: "restore",
        id: "backup-verified",
      });
    });
  });

  it("fails closed when no verified recovery point is available", async () => {
    const user = userEvent.setup();
    useDesktopPayload(false);
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    expect(screen.getByRole("button", { name: "批准" })).toBeDisabled();
    expect(screen.getByText("需要已验证的回滚点后才能批准此变更。")).toBeVisible();

    await user.click(screen.getByRole("tab", { name: "备份与回滚" }));
    expect(await screen.findByRole("button", { name: "回滚到此处" })).toBeDisabled();
  });

  it("fails closed when a recovery point lacks impact details", async () => {
    const payload = desktopPayload(true);
    payload.backupPoints[0].files = 0;
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_data") return payload;
      return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
    });
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    expect(screen.getByRole("button", { name: "批准" })).toBeDisabled();
    expect(screen.getByText("需要已验证的回滚点后才能批准此变更。")).toBeVisible();
  });

  it("keeps a change pending when the native approval reports failure", async () => {
    const payload = desktopPayload(true);
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_data") return payload;
      if (command === "preview_route_profile_draft_apply") return routeReview;
      if (command === "apply_route_profile_draft") {
        return { ok: false, summary: "回滚点写入失败", logs: [], tools: [] };
      }
      return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
    });
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    fireEvent.click(screen.getByRole("button", { name: "批准" }));
    const dialog = await screen.findByRole("alertdialog", { name: "审阅并批准路由" });
    fireEvent.click(within(dialog).getByRole("button", { name: "确认应用" }));

    expect(await screen.findByText("路由应用未完成：回滚点写入失败")).toBeVisible();
    expect(screen.getByText("D:/d-token/routing/route-a.json")).toBeVisible();
  });

  it("requires a new review after the route draft or active config becomes stale", async () => {
    const payload = desktopPayload(true);
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_data") return payload;
      if (command === "preview_route_profile_draft_apply") return routeReview;
      if (command === "apply_route_profile_draft") {
        return { ok: false, summary: "Route review is stale; review the current diff again", logs: [], tools: [] };
      }
      return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
    });
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    fireEvent.click(screen.getByRole("button", { name: "批准" }));
    const dialog = await screen.findByRole("alertdialog", { name: "审阅并批准路由" });
    fireEvent.click(within(dialog).getByRole("button", { name: "确认应用" }));

    expect(await screen.findByText("应用已阻止：草稿或活动配置在审阅后发生变化，请重新审阅当前差异。")).toBeVisible();
    expect(screen.getByText("D:/d-token/routing/route-a.json")).toBeVisible();
    expect(screen.getByRole("button", { name: "批准" })).toBeEnabled();
  });

  it("never opens confirmation or calls apply when the review lacks hash evidence", async () => {
    const payload = desktopPayload(true);
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_data") return payload;
      if (command === "preview_route_profile_draft_apply") return { ...routeReview, after_sha256: "" };
      return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
    });
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    fireEvent.click(screen.getByRole("button", { name: "批准" }));

    expect(await screen.findByText("无法生成路由审阅：桌面控制面未返回完整差异与 SHA-256 证据。")).toBeVisible();
    expect(screen.queryByRole("alertdialog", { name: "审阅并批准路由" })).not.toBeInTheDocument();
    expect(native.invokeNative.mock.calls.some(([command]) => command === "apply_route_profile_draft")).toBe(false);
  });
});
