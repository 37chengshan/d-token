import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
  RUNTIME_INVALIDATED_EVENT: "d-token:runtime-invalidated",
}));

import { Changes } from "./Changes";

const payload = {
  desktop_revision: "changes-iteration-test-revision",
  source: "dtoken-desktop-control-plane",
  desktop_data_source: "dtoken-desktop-control-plane",
  freshness: "fresh",
  complete: true,
  transaction_provenance: {
    source: "dtoken-desktop-control-plane",
    command: "get_config_change_records",
    schema_version: 1,
    runtime_required: false,
    generated_at: "2026-07-15T10:00:00Z",
  },
  routeDrafts: [{ path: "route-a.json", config: { routes: [{ id: "route-a" }] } }],
  backupPoints: [],
  desktopTransactionEvents: [],
};

beforeEach(() => {
  native.invokeNative.mockReset();
  native.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_config_change_records") return payload;
    throw new Error(`unexpected command: ${command}`);
  });
});

afterEach(() => {
  cleanup();
  window.sessionStorage.clear();
});

describe("Changes information architecture", () => {
  it.each([null, "pending", "diff"])("opens Backup & Rollback for legacy session value %s", async (legacyTab) => {
    if (legacyTab) window.sessionStorage.setItem("d-token.changes.tab", legacyTab);

    render(<Changes />);

    const backupTab = await screen.findByRole("tab", { name: "备份与回滚" });
    expect(backupTab).toHaveAttribute("data-state", "active");
    expect(screen.queryByRole("tab", { name: "待批变更" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "配置 Diff" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "变更记录" })).toBeVisible();
  });

  it("keeps unfinished drafts in their source workflow instead of approving them here", async () => {
    render(<Changes />);

    expect(await screen.findByText("有 1 项未完成配置")).toBeVisible();
    expect(screen.getByText(/请返回安装与连接流程查看当前变更/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "批准" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "拒绝" })).not.toBeInTheDocument();
  });
});
