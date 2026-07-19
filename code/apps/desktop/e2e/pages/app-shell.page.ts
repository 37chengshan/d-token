import type { Locator, Page } from "@playwright/test";

export const MAIN_PAGES = [
  { id: "overview", navLabel: "总体概览", heading: "概览", english: { navLabel: "Overview", heading: "Overview" } },
  { id: "install", navLabel: "安装与连接", heading: "安装与连接", english: { navLabel: "Install & Connect", heading: "Install & Connect" } },
  { id: "codebase-memory", navLabel: "代码图谱", heading: "代码图谱（实验性）", english: { navLabel: "Code Graph", heading: "Code Graph (Experimental)" } },
  { id: "analytics", navLabel: "分析", heading: "分析", english: { navLabel: "Analytics", heading: "Analytics" } },
  { id: "visual", navLabel: "视觉压缩 Beta", heading: "视觉压缩", english: { navLabel: "Visual Compression Beta", heading: "Visual Compression" } },
  { id: "doctor", navLabel: "Doctor", heading: "Doctor", english: { navLabel: "Doctor", heading: "Doctor" } },
  { id: "changes", navLabel: "变更与回退", heading: "变更与回退", english: { navLabel: "Changes & Rollback", heading: "Changes & Rollback" } },
  { id: "settings", navLabel: "设置", heading: "系统设置", english: { navLabel: "Settings", heading: "Settings" } },
] as const;

export type MainPage = (typeof MAIN_PAGES)[number];

export class AppShellPage {
  readonly page: Page;
  readonly main: Locator;
  readonly root: Locator;

  constructor(page: Page) {
    this.page = page;
    this.main = page.locator("main");
    this.root = page.locator("html");
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

  async useEnglish() {
    await this.navigateTo(MAIN_PAGES.find((item) => item.id === "settings")!);
    await this.main.getByRole("radio", { name: "English", exact: true }).click();
    await this.heading("Settings").waitFor();
  }

  async navigateToEnglish(target: MainPage) {
    const navItem = this.navItem(target.english.navLabel);
    await navItem.click();
    await this.heading(target.english.heading).waitFor();
  }

  async useCompactViewport() {
    await this.page.setViewportSize({ width: 920, height: 640 });
  }

  async documentWidth() {
    return this.page.evaluate(() => ({
      clientWidth: document.documentElement.clientWidth,
      scrollWidth: document.documentElement.scrollWidth,
    }));
  }

  async selectDarkAppearance() {
    const dark = this.main.getByRole("radio", { name: "深色", exact: true });
    await dark.click();
    return dark;
  }

  async surfaceState() {
    return this.page.evaluate(() => {
      const shell = document.querySelector(".app-shell");
      const style = shell ? window.getComputedStyle(shell) : null;
      return {
        fieldCount: document.querySelectorAll(".background-flow-field, .background-atmosphere").length,
        legacyLayerCount: document.querySelectorAll(".background-stars, .background-nebula").length,
        backgroundImage: style?.backgroundImage ?? "missing",
        backgroundColor: style?.backgroundColor ?? "missing",
      };
    });
  }

  async reducedMotionState() {
    return this.page.evaluate(() => {
      const relevantSelector = ".shell-page, .shell-page-stage, .animated-nav-icon .icon-part";
      const relevantElements = Array.from(document.querySelectorAll(relevantSelector));
      const runningAnimations = document.getAnimations().filter((animation) => {
        const target = (animation.effect as KeyframeEffect | null)?.target;
        return (
          target instanceof Element &&
          target.matches(relevantSelector) &&
          (animation.playState === "running" || animation.playState === "pending")
        );
      });

      return {
        preferenceMatches: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
        animationNames: relevantElements.map((element) => window.getComputedStyle(element).animationName),
        runningCount: runningAnimations.length,
      };
    });
  }

  async focusFirstPrimaryNavigation(useMacFullKeyboardAccessChord = false) {
    const target = this.navItem(MAIN_PAGES[0].navLabel);
    const chord = useMacFullKeyboardAccessChord ? "Alt+Tab" : "Tab";
    for (let attempt = 0; attempt < 20; attempt += 1) {
      await this.page.keyboard.press(chord);
      if (await target.evaluate((element) => document.activeElement === element)) break;
    }
    return target;
  }

  async hasVisibleFocus(locator: Locator) {
    return locator.evaluate((element) => {
      const style = window.getComputedStyle(element);
      return (
        document.activeElement === element &&
        style.outlineStyle !== "none" &&
        Number.parseFloat(style.outlineWidth) > 0
      );
    });
  }
}
