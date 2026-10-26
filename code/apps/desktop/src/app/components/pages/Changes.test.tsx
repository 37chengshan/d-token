import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({ invokeNative: native.invokeNative, RUNTIME_INVALIDATED_EVENT: "d-token:runtime-invalidated" }));

import { Changes } from "./Changes";

const currentSha = "a".repeat(64);

function desktopPayload(overrides: Record<string, unknown> = {}) {
  return {
    desktop_revision: "changes-test-revision",
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
    routeDrafts: [],
    backupPoints: [
      {
        id: "backup-verified",
        label: "Rollback point d-token active config",
        when: "2026-07-15 10:00",
        files: 1,
        target: "/safe/d-token-config.json",
        scope: "dtoken-active-config",
        verified: true,
        integrity_verified: true,
        currently_restorable: true,
        expected_current_sha256: currentSha,
        blocked_reason: "",
      },
    ],
    desktopTransactionEvents: [],
    ...overrides,
  };
}

function mockDesktop(payload = desktopPayload()) {
  native.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_config_change_records") return payload;
    if (command === "preview_rollback") {
      return {
        ok: true,
        mode: "rollback-preview",
        revision: "rollback-rev-1",
        rollback_id: "backup-verified",
        scope: "dtoken-active-config",
        target: "/safe/d-token-config.json",
        files: 1,
        integrity_verified: true,
        currently_restorable: true,
        blocked_reason: "",
        expected_current_sha256: currentSha,
        summary: "verified",
      };
    }
    if (command === "apply_rollback") {
      return {
        ok: true,
        mode: "rollback-receipt",
        rollback_id: "backup-verified",
        summary: "restored",
        verification_state: "verified",
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });
}

beforeEach(() => {
  native.invokeNative.mockReset();
  window.sessionStorage.clear();
});

afterEach(cleanup);

describe("Changes rollback safety", () => {
  it.each([
    { integrity_verified: false, verified: true },
    { scope: "unknown" },
  ])("keeps conflicting or unknown backup metadata locked: %j", async (invalid) => {
    mockDesktop(desktopPayload({ backupPoints: [{ ...desktopPayload().backupPoints[0], ...invalid }] }));
    render(<Changes />);
    expect(await screen.findByRole("button", { name: "预览回退" })).toBeDisabled();
    expect(native.invokeNative).toHaveBeenCalledWith("get_config_change_records");
    expect(native.invokeNative).not.toHaveBeenCalledWith("get_desktop_data");
  });

  it("keeps a verified tool backup available without requiring runtime metrics", async () => {
    mockDesktop(desktopPayload({ backupPoints: [{ ...desktopPayload().backupPoints[0], scope: "managed-agent-config" }] }));
    render(<Changes />);
    expect(await screen.findByRole("button", { name: "预览回退" })).toBeEnabled();
    expect(screen.getByRole("heading", { name: "工具配置恢复点" })).toBeVisible();
  });

  it("retains records but locks confirmation when a refresh fails", async () => {
    mockDesktop();
    render(<Changes />);
    fireEvent.click(await screen.findByRole("button", { name: "预览回退" }));
    await screen.findByRole("alertdialog");
    native.invokeNative.mockRejectedValue(new Error("read_failed"));
    await act(async () => { window.dispatchEvent(new Event("d-token:runtime-invalidated")); });
    expect(screen.getByRole("button", { name: "确认回退并复检" })).toBeDisabled();
    expect(screen.getByRole("alertdialog")).toHaveTextContent("数据已过期");
  });

  it("distinguishes a failed local read from an empty backup list", async () => {
    native.invokeNative.mockRejectedValue(new Error("read_failed"));
    render(<Changes />);
    expect(await screen.findByText("恢复记录暂时无法读取，请刷新后重试。")).toBeVisible();
    expect(screen.queryByText("当前没有可用回滚点")).not.toBeInTheDocument();
  });

  it("keeps browser preview view-only and free of executable rollback actions", async () => {
    native.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    render(<Changes />);

    expect(await screen.findByText(/浏览器预览仅展示结构/)).toBeVisible();
    expect(screen.getByText("当前没有可用回滚点")).toBeVisible();
    expect(screen.queryByRole("button", { name: "预览回退" })).not.toBeInTheDocument();
  });

  it("distinguishes backup integrity from current restorability", async () => {
    const payload = desktopPayload({
      backupPoints: [{
        ...desktopPayload().backupPoints[0],
        currently_restorable: false,
        blocked_reason: "The current target hash changed after this rollback point was created.",
      }],
    });
    mockDesktop(payload);
    render(<Changes />);

    expect(await screen.findByText("元数据与备份内容已验证")).toBeVisible();
    expect(screen.getByText("文件在备份后发生了变化。请先查看当前配置，避免覆盖后续修改。")).toBeVisible();
    expect(screen.getByRole("button", { name: "预览回退" })).toBeDisabled();
  });

  it("previews current hashes before requiring explicit rollback confirmation", async () => {
    mockDesktop();
    render(<Changes />);

    fireEvent.click(await screen.findByRole("button", { name: "预览回退" }));
    const dialog = await screen.findByRole("alertdialog", { name: "确认回退" });
    expect(within(dialog).getByText(/先备份当前状态/)).toBeVisible();
    expect(native.invokeNative).toHaveBeenCalledWith("preview_rollback", {
      request: { rollback_id: "backup-verified" },
    });
    expect(native.invokeNative).not.toHaveBeenCalledWith("apply_rollback", expect.anything());

    fireEvent.click(within(dialog).getByRole("button", { name: "确认回退并复检" }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("apply_rollback", {
        request: {
          rollback_id: "backup-verified",
          reviewed_revision: "rollback-rev-1",
          expected_current_sha256: currentSha,
          confirmed: true,
        },
      });
    });
    expect(await screen.findByText("回退完成，文件与运行时复检均已通过。")).toBeVisible();
  });

  it("renders an Agent transaction as one rollback point and uses the centralized source dispatcher", async () => {
    const transactionToken = "transaction-agent-current";
    const payload = desktopPayload({
      backupPoints: [{
        ...desktopPayload().backupPoints[0],
        id: "agent-rollback-primary",
        label: "Agent integration transaction tx-agent-1",
        files: 3,
        target: "3 managed Agent target(s)",
        scope: "agent-integration",
        executor_kind: "agent_integration",
        transaction_id: "tx-agent-1",
        expected_current_sha256: transactionToken,
      }],
    });
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_config_change_records") return payload;
      if (command === "preview_rollback") {
        return {
          ok: true,
          mode: "rollback-preview",
          revision: "rollback-agent-rev-1",
          rollback_id: "agent-rollback-primary",
          scope: "agent-integration",
          executor_kind: "agent_integration",
          transaction_id: "tx-agent-1",
          target: "3 managed Agent target(s)",
          files: 3,
          integrity_verified: true,
          currently_restorable: true,
          blocked_reason: "",
          expected_current_sha256: transactionToken,
          summary: "verified",
        };
      }
      if (command === "apply_rollback") {
        return {
          ok: true,
          mode: "rollback-receipt",
          rollback_id: "agent-rollback-primary",
          transaction_id: "tx-agent-1",
          summary: "restored",
          verification_state: "verified",
        };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<Changes />);

    await screen.findByText(/3 个文件/);
    fireEvent.click(screen.getByText("文件与记录信息"));
    expect(screen.getByText("tx-agent-1")).toBeVisible();
    expect(screen.getByText(/3 个文件/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "预览回退" }));
    const dialog = await screen.findByRole("alertdialog", { name: "确认回退" });
    expect(within(dialog).getByText("tx-agent-1")).toBeVisible();
    fireEvent.click(within(dialog).getByRole("button", { name: "确认回退并复检" }));

    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("apply_rollback", {
        request: {
          rollback_id: "agent-rollback-primary",
          reviewed_revision: "rollback-agent-rev-1",
          expected_current_sha256: transactionToken,
          confirmed: true,
        },
      });
    });
    expect(native.invokeNative.mock.calls.some(([command]) => command === "rollback_hybrid_agent_integration")).toBe(false);
  });

  it("fails closed when rollback preview says the current state changed", async () => {
    mockDesktop();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_config_change_records") return desktopPayload();
      if (command === "preview_rollback") {
        return {
          ok: false,
          mode: "rollback-preview",
          rollback_id: "backup-verified",
          integrity_verified: true,
          currently_restorable: false,
          blocked_reason: "Current hash changed",
        };
      }
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Changes />);

    fireEvent.click(await screen.findByRole("button", { name: "预览回退" }));
    expect(await screen.findByText("当前回滚点已不可恢复，请刷新后重试。")).toBeVisible();
    expect(screen.queryByRole("alertdialog", { name: "确认回退" })).not.toBeInTheDocument();
    expect(native.invokeNative.mock.calls.some(([command]) => command === "apply_rollback")).toBe(false);
  });

  it("keeps rollback confirmation open with an inline retry error when apply fails", async () => {
    mockDesktop();
    native.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_config_change_records") return desktopPayload();
      if (command === "preview_rollback") {
        return {
          ok: true,
          mode: "rollback-preview",
          revision: "rollback-rev-1",
          rollback_id: "backup-verified",
          scope: "dtoken-active-config",
          target: "/safe/d-token-config.json",
          files: 1,
          integrity_verified: true,
          currently_restorable: true,
          blocked_reason: "",
          expected_current_sha256: currentSha,
          summary: "verified",
        };
      }
      if (command === "apply_rollback") {
        return {
          ok: false,
          mode: "rollback-receipt",
          rollback_id: "backup-verified",
          summary: "rollback failed",
          verification_state: "failed",
        };
      }
      throw new Error(`unexpected command: ${command}`);
    });
    render(<Changes />);

    fireEvent.click(await screen.findByRole("button", { name: "预览回退" }));
    const dialog = await screen.findByRole("alertdialog", { name: "确认回退" });
    fireEvent.click(within(dialog).getByRole("button", { name: "确认回退并复检" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("请重试");
    expect(dialog).toBeInTheDocument();
    expect(native.invokeNative.mock.calls.filter(([command]) => command === "apply_rollback")).toHaveLength(1);
  });

  it("locks actions when the desktop snapshot is stale", async () => {
    mockDesktop(desktopPayload({ data_status: "stale" }));
    render(<Changes />);

    expect(await screen.findByText(/数据已过期，不能证明当前文件状态/)).toBeVisible();
    expect(screen.getByRole("button", { name: "预览回退" })).toBeDisabled();
  });

  it("shows committed persistent changes but excludes service and preview events", async () => {
    const user = userEvent.setup();
    mockDesktop(desktopPayload({
      desktopTransactionEvents: [
        { timestamp: "2026-07-15T10:00:00Z", stage: "service_restart", status: "ok", metadata: {} },
        { timestamp: "2026-07-15T10:01:00Z", stage: "install_scan_plan_diff", status: "ok", metadata: {} },
        { timestamp: "2026-07-15T10:02:00Z", stage: "install_backup_apply_verify_doctor_commit", status: "ok", metadata: { rollback_id: "rb-1", target: "OpenCode", diff: "--- before\n+++ after" } },
      ],
    }));
    render(<Changes />);

    await user.click(await screen.findByRole("tab", { name: "变更记录" }));
    expect(screen.getByText("安装与连接配置已提交")).toBeVisible();
    expect(screen.getByText("rb-1")).toBeVisible();
    expect(screen.getByText("查看变更详情")).toBeVisible();
    expect(screen.queryByText("service_restart")).not.toBeInTheDocument();
    expect(screen.queryByText("install_scan_plan_diff")).not.toBeInTheDocument();
  });
});
