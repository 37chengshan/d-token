import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
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

  it("does not fabricate browser candidates when the native scanner is absent", () => {
    nativeRuntime.isTauriRuntime.mockReturnValue(false);
    render(<SmartLearning />);

    expect(
      screen.getByText("本机扫描与受控写入尚未接通，当前不会扫描会话或修改文件。"),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "本机扫描尚未接通" })).toBeDisabled();
    expect(screen.queryByText("D:/D-TOKEN/d-token")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "查看预览样例" })).not.toBeInTheDocument();
  });
});
