import React from "react";
import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const nativeRuntime = vi.hoisted(() => ({ isTauriRuntime: vi.fn() }));

vi.mock("../../lib/native", () => ({
  isTauriRuntime: nativeRuntime.isTauriRuntime,
}));

import { SmartLearning } from "./SmartLearning";

beforeEach(() => {
  nativeRuntime.isTauriRuntime.mockReturnValue(true);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("SmartLearning", () => {
  it("does not pretend that native scanning is available", () => {
    render(<SmartLearning />);

    expect(
      screen.getByText("本机扫描与受控写入尚未接通，当前不会扫描会话或修改文件。"),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "本机扫描尚未接通" })).toBeDisabled();
    expect(screen.queryByText("已写入 d-token 管理区块，其他内容未改动。")).not.toBeInTheDocument();
  });

  it("keeps browser candidates explicitly read-only", () => {
    nativeRuntime.isTauriRuntime.mockReturnValue(false);
    vi.useFakeTimers();
    render(<SmartLearning />);

    expect(
      screen.getByText("浏览器预览：候选目录是示例，不会扫描或写入本机。"),
    ).toBeVisible();
    expect(
      screen.queryByText("还没有扫描结果。开始全面扫描后，这里会显示按学习价值排序的候选目录。"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看预览样例" }));
    act(() => vi.advanceTimersByTime(900));

    const candidatePath = screen.getByText("D:/D-TOKEN/d-token");
    fireEvent.click(candidatePath.closest("button")!);
    expect(screen.getByRole("button", { name: "仅预览，不会写入" })).toBeDisabled();
  });
});
