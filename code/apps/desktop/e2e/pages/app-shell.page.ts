import type { Locator, Page } from "@playwright/test";

export const MAIN_PAGES = [
  { id: "overview", navLabel: "总体概览", heading: "概览" },
  { id: "install", navLabel: "安装与连接", heading: "安装与连接" },
  { id: "analytics", navLabel: "分析", heading: "分析" },
  { id: "doctor", navLabel: "Doctor", heading: "诊断" },
  { id: "changes", navLabel: "变更与回退", heading: "变更与回滚" },
  { id: "settings", navLabel: "设置", heading: "系统设置" },
] as const;

export type MainPage = (typeof MAIN_PAGES)[number];

export class AppShellPage {
  readonly page: Page;
  readonly main: Locator;

  constructor(page: Page) {
    this.page = page;
    this.main = page.locator("main");
  }

  async open() {
    await this.page.goto("/");
    await this.main.getByRole("heading", { level: 1, name: "概览", exact: true }).waitFor();
  }

  navItem(label: string) {
    return this.page.getByRole("button", { name: label, exact: true });
  }

  heading(label: string) {
    return this.main.getByRole("heading", { level: 1, name: label, exact: true });
  }

  async navigateTo(target: MainPage) {
    const navItem = this.navItem(target.navLabel);
    await navItem.click();
    await this.heading(target.heading).waitFor();
  }
}
