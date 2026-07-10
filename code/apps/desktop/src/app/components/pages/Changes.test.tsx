import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: native.invokeNative,
}));

import { Changes } from "./Changes";

const desktopPayload = (verified = true) => ({
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
      verified,
    },
  ],
});

function useDesktopPayload(verified = true) {
  const payload = desktopPayload(verified);
  native.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_desktop_data") return payload;
    return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
  });
}

beforeEach(() => {
  native.invokeNative.mockReset();
});

afterEach(cleanup);

describe("Changes action safety", () => {
  it("keeps approve, reject and rollback disabled in browser preview", async () => {
    const user = userEvent.setup();
    native.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    render(<Changes />);

    expect(await screen.findByText("浏览器预览仅供查看，批准、拒绝和回退均不可执行。")).toBeVisible();
    for (const button of screen.getAllByRole("button", { name: "批准" })) {
      expect(button).toBeDisabled();
    }
    for (const button of screen.getAllByRole("button", { name: "拒绝" })) {
      expect(button).toBeDisabled();
    }

    await user.click(screen.getByRole("tab", { name: "备份与回滚" }));
    for (const button of await screen.findAllByRole("button", { name: "回滚到此处" })) {
      expect(button).toBeDisabled();
    }
  });

  it("shows impact, backup and recovery information before approving a real change", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    useDesktopPayload(true);
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    fireEvent.click(screen.getByRole("button", { name: "批准" }));

    const dialog = await screen.findByRole("alertdialog", { name: "确认批准变更" });
    expect(within(dialog).getByText("影响范围")).toBeVisible();
    expect(within(dialog).getByText("D:/d-token/routing/route-a.json")).toBeVisible();
    expect(within(dialog).getByText("备份保障")).toBeVisible();
    expect(within(dialog).getByText(/backup-verified/)).toBeVisible();
    expect(within(dialog).getByText("恢复方式")).toBeVisible();
    expect(within(dialog).getByText(/回退/)).toBeVisible();
    expect(native.invokeNative).not.toHaveBeenCalledWith("apply_route_profile_draft", expect.anything());

    fireEvent.click(within(dialog).getByRole("button", { name: "确认批准" }));
    await waitFor(() => {
      expect(native.invokeNative).toHaveBeenCalledWith("apply_route_profile_draft", {
        request: { route_id: "route-a" },
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
      if (command === "apply_route_profile_draft") {
        return { ok: false, summary: "回滚点写入失败", logs: [], tools: [] };
      }
      return { ok: true, summary: `${command} complete`, logs: [], tools: [] };
    });
    render(<Changes />);
    await screen.findByText("D:/d-token/routing/route-a.json");

    fireEvent.click(screen.getByRole("button", { name: "批准" }));
    const dialog = await screen.findByRole("alertdialog", { name: "确认批准变更" });
    fireEvent.click(within(dialog).getByRole("button", { name: "确认批准" }));

    expect(await screen.findByText("回滚点写入失败")).toBeVisible();
    expect(screen.getByText("D:/d-token/routing/route-a.json")).toBeVisible();
  });
});
