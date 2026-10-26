import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => { cleanup(); window.sessionStorage.clear(); resetInstallMachineState(); });

vi.mock("./Install", () => ({ Install: ({ initialAgentId }: { initialAgentId?: string }) => <div>一键安装内容:{initialAgentId ?? "none"}</div> }));
vi.mock("./SmartLearning", () => ({ SmartLearning: () => <div>智能学习内容</div> }));
vi.mock("./Advanced", () => ({ Advanced: () => <div>高级安装内容</div> }));
vi.mock("./LogsSecurity", () => ({ LogsSecurity: () => <div>日志内容</div> }));

import { InstallWorkbench } from "./InstallWorkbench";
import { hasPendingInstallHandoff, requestInstallHandoff, resetInstallMachineState } from "../../lib/installMachine";

describe("InstallWorkbench", () => {
  it("renders a full-width four-column task navigation", () => {
    render(<InstallWorkbench />);

    const tablist = screen.getByRole("tablist");
    expect(tablist).toHaveClass("install-workbench-nav", "grid", "h-10", "w-full", "grid-cols-4");
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
      expect(within(tab).queryByText(description)).not.toBeInTheDocument();
    }
  });

  it("forwards the onboarding Agent selection to one-click install", () => {
    render(<InstallWorkbench initialAgentId="codex" />);

    expect(screen.getByText("一键安装内容:codex")).toBeInTheDocument();
  });

  it("opens the connection panel for an authorized handoff despite the remembered tab", async () => {
    window.sessionStorage.setItem("d-token.install.workbench-tab", "advanced");
    requestInstallHandoff({ agentIds: ["opencode"], discoveryRevision: "r1", source: "onboarding-start-setup" });
    render(<InstallWorkbench />);
    expect(screen.getByRole("tabpanel")).toHaveTextContent("一键安装内容");
    await userEvent.setup().click(screen.getByRole("tab", { name: /高级安装/ }));
    expect(hasPendingInstallHandoff()).toBe(false);
  });

  it("retains the remembered panel during ordinary navigation", async () => {
    window.sessionStorage.setItem("d-token.install.workbench-tab", "advanced");
    render(<InstallWorkbench />);
    await waitFor(() => expect(screen.getByRole("tabpanel")).toHaveTextContent("高级安装内容"));
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
