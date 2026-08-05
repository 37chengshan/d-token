import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(cleanup);

vi.mock("./Install", () => ({ Install: () => <div>一键安装内容</div> }));
vi.mock("./SmartLearning", () => ({ SmartLearning: () => <div>智能学习内容</div> }));
vi.mock("./Advanced", () => ({ Advanced: () => <div>高级安装内容</div> }));
vi.mock("./LogsSecurity", () => ({ LogsSecurity: () => <div>日志内容</div> }));

import { InstallWorkbench } from "./InstallWorkbench";

describe("InstallWorkbench", () => {
  it("renders a full-width four-column task navigation", () => {
    render(<InstallWorkbench />);

    const tablist = screen.getByRole("tablist");
    expect(tablist).toHaveClass("install-workbench-nav", "grid", "h-20", "w-full", "grid-cols-4");
    expect(screen.getAllByRole("tab")).toHaveLength(4);

    const expectedTabs = [
      ["一键安装", "启动服务并连接常用 Agent"],
      ["智能学习", "扫描可复用配置，确认后再应用"],
      ["高级安装", "自定义上游、连接路径与实例"],
      ["日志", "查看安装、连接与诊断记录"],
    ];
    for (const [title, description] of expectedTabs) {
      const tab = screen.getByRole("tab", { name: new RegExp(title) });
      expect(within(tab).getByText(title)).toBeVisible();
      const descriptionNode = within(tab).getByText(description);
      expect(descriptionNode).toBeVisible();
      expect(descriptionNode).toHaveClass("install-workbench-nav-description");
      expect(descriptionNode).not.toHaveClass("truncate");
    }
  });

  it("switches panels through accessible tabs", async () => {
    const user = userEvent.setup();
    render(<InstallWorkbench />);

    const smartLearning = screen.getByRole("tab", { name: /智能学习/ });
    await user.click(smartLearning);

    expect(smartLearning).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(screen.getByRole("tabpanel")).toHaveTextContent("智能学习内容"));
  });
});
