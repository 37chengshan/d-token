import { expect, test, type Locator } from "@playwright/test";
import { AppShellPage, MAIN_PAGES } from "./pages/app-shell.page";

const FORBIDDEN_PRODUCT_TEST_LANGUAGE =
  /workload\s+benchmark|smoke\s+test|testing\s+center|test\s+center|fixture|mock|fake[-\s]?bin|\bsample\b|\bdemo\b|测试中心|冒烟测试|基准测试|测试夹具|模拟(?:数据|服务)|假(?:数据|服务)|示例|演示/i;
const FORBIDDEN_APPROVAL_LANGUAGE = /审查|审阅|审核|\b(?:review(?:ed|ing|able)?|audit)\b/i;
const FORBIDDEN_AI_STYLE_LANGUAGE = /让我们|我来|为你(?:准备|处理|生成)|无需担心|轻松(?:完成|管理)|\b(?:let'?s|no worries|effortlessly|excited to)\b/i;

test.describe("d-token desktop control plane", () => {
  test("all primary pages are reachable from the shared shell", async ({ page }) => {
    const app = new AppShellPage(page);
    await app.open();

    for (const target of MAIN_PAGES) {
      await test.step(target.heading, async () => {
        await app.navigateTo(target);
        await expect(app.navItem(target.navLabel)).toHaveAttribute("aria-current", "page");
        await expect(app.heading(target.heading)).toBeVisible();
        const surfaceStructure = await app.main.evaluate((main) => ({
          nestedCount: main.querySelectorAll("[data-surface] [data-surface]").length,
          primaryCount: main.querySelectorAll('[data-surface="primary"], [data-surface="metric-strip"]').length,
        }));
        expect(surfaceStructure.nestedCount, `${target.heading} must not nest product surfaces`).toBe(0);
        expect(surfaceStructure.primaryCount, `${target.heading} must use at most two bounded primary surfaces`).toBeLessThanOrEqual(2);
      });
    }
  });

  test("browser preview fails closed for native detection and change mutations", async ({ page }) => {
    const unexpectedExternalRequests: string[] = [];
    page.on("request", (request) => {
      const url = new URL(request.url());
      if (url.protocol === "http:" || url.protocol === "https:") {
        if (url.hostname !== "127.0.0.1" && url.hostname !== "localhost") {
          unexpectedExternalRequests.push(request.url());
        }
      }
    });

    const app = new AppShellPage(page);
    await app.open();
    expect(await page.evaluate(() => !("__TAURI_INTERNALS__" in window))).toBe(true);

    await app.navigateTo(MAIN_PAGES[1]);
    await expect(app.main.getByText("需要处理", { exact: true }).first()).toBeVisible();
    await app.main.getByRole("button", { name: "重新扫描", exact: true }).click();
    await expect(
      app.main.getByText("当前是浏览器预览，不读取或改写本机配置。", {
        exact: true,
      }),
    ).toBeVisible();

    await app.navigateTo(MAIN_PAGES.find((item) => item.id === "changes")!);
    await expect(app.main.getByText("浏览器预览仅展示结构，不提供本机备份证据，回退已锁定。", { exact: true })).toBeVisible();
    await expect(app.main.getByText("当前没有可用回滚点", { exact: true })).toBeVisible();
    await expect(app.main.getByRole("button", { name: "批准", exact: true })).toHaveCount(0);
    await expect(app.main.getByRole("button", { name: "拒绝", exact: true })).toHaveCount(0);

    await expect(app.main.getByRole("button", { name: "回滚到此处", exact: true })).toHaveCount(0);
    expect(unexpectedExternalRequests).toEqual([]);
  });

  test("unavailable native and destructive settings remain disabled in preview", async ({ page }) => {
    const app = new AppShellPage(page);
    await app.open();
    await app.navigateTo(MAIN_PAGES.find((item) => item.id === "settings")!);
    await expect(app.main.getByText("浏览器预览", { exact: true }).first()).toBeVisible();

    await expect(app.main.getByRole("button", { name: /^(启动服务|停止服务)$/ })).toHaveCount(0);
    await expect(app.main.getByRole("switch", { name: "自动启动 d-token 服务", exact: true })).toBeDisabled();
    await expect(app.main.getByText("浏览器预览", { exact: true })).toHaveCount(2);
    await expect(app.main.getByRole("button", { name: "检查更新", exact: true })).toBeDisabled();
    await expect(app.main.getByText("只接受经过签名验证的发布源；当前未配置发布源时会明确显示不可用。", { exact: true })).toBeVisible();
    await expect(app.main.getByRole("switch", { name: "诊断数据上传", exact: true })).toHaveCount(0);
    await expect(app.main.getByText("高隐藏 · 上传关闭", { exact: true })).toBeVisible();
    for (const label of ["智能学习全面扫描", "自动检查更新"]) {
      await expect(app.main.getByRole("switch", { name: label, exact: true })).toBeDisabled();
    }
    await expect(app.main.getByRole("radio", { name: "深入", exact: true })).toBeDisabled();
    await expect(app.main.getByRole("button", { name: "管理范围", exact: true })).toHaveCount(0);
    await expect(app.main.getByText("本地聚合指标学习；项目扫描在「安装与连接 → 智能学习」。", { exact: true })).toBeVisible();
  });

  test("production-facing pages and workbench tabs exclude development test vocabulary", async ({ page }) => {
    const app = new AppShellPage(page);
    await app.open();

    for (const target of MAIN_PAGES) {
      await app.navigateTo(target);
      await expectProductCopy(app.main, target.heading);
    }

    await app.navigateTo(MAIN_PAGES[1]);
    for (const tab of ["一键安装", "智能学习", "高级安装", "日志"]) {
      await app.main.getByRole("tab", { name: tab }).click();
      await expectProductCopy(app.main, `安装与连接 / ${tab}`);
    }

    await app.navigateTo(MAIN_PAGES.find((item) => item.id === "doctor")!);
    await expect(app.main.getByRole("button", { name: "扫描常规问题", exact: true })).toBeVisible();
    await expectProductCopy(app.main, "Doctor");
  });

  test("English product pages and workbench tabs use the same release-ready language", async ({ page }) => {
    const app = new AppShellPage(page);
    await app.open();
    await app.useEnglish();

    for (const target of MAIN_PAGES) {
      await app.navigateToEnglish(target);
      await expectProductCopy(app.main, `English / ${target.english.heading}`);
    }

    await app.navigateToEnglish(MAIN_PAGES[1]);
    for (const tab of ["One-Click Install", "Smart Learning", "Advanced Setup", "Logs"]) {
      await app.main.getByRole("tab", { name: tab }).click();
      await expectProductCopy(app.main, `Install & Connect / ${tab}`);
    }
  });

  test("compact desktop viewport keeps every primary page inside the document width", async ({ page }) => {
    const app = new AppShellPage(page);
    await app.useCompactViewport();
    await app.open();

    for (const target of MAIN_PAGES) {
      await test.step(target.heading, async () => {
        await app.navigateTo(target);
        const width = await app.documentWidth();
        expect(width.scrollWidth, `${target.heading} must not create horizontal document overflow`).toBeLessThanOrEqual(
          width.clientWidth,
        );
      });
    }
  });

  test("overview has one page title and no installation entry CTA", async ({ page }) => {
    const app = new AppShellPage(page);
    await app.open();

    await expect(app.main.getByRole("heading", { level: 1 })).toHaveCount(1);
    await expect(app.main.getByText("建议下一步", { exact: true })).toHaveCount(0);
    await expect(app.main.getByRole("radio", { name: "年", exact: true })).toHaveAttribute("aria-checked", "true");
    await expect(app.main.locator('[data-testid="activity-year"]')).toBeVisible();

    const functionEntryName =
      /^(?:前往安装与连接|安装\s*\/\s*配置 Agent|开始配置|启动并连接 d-token|打开 Doctor|查看分析)$/;
    const functionEntry = app.main
      .getByRole("button", { name: functionEntryName })
      .or(app.main.getByRole("link", { name: functionEntryName }));
    await expect(functionEntry).toHaveCount(0);
  });

  test("dark appearance stays selected in preview without a native persistence claim", async ({ page }) => {
    const app = new AppShellPage(page);
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await app.open();
    const surface = await app.surfaceState();
    expect(surface.fieldCount).toBe(0);
    expect(surface.legacyLayerCount).toBe(0);
    expect(surface.backgroundImage).toBe("none");
    expect(surface.backgroundColor).not.toBe("rgba(0, 0, 0, 0)");
    await app.navigateTo(MAIN_PAGES.find((item) => item.id === "settings")!);
    await expect(app.main.getByText("浏览器预览", { exact: true }).first()).toBeVisible();

    const dark = await app.selectDarkAppearance();
    await expect(dark).toHaveAttribute("aria-checked", "true");
    await expect(app.root).toHaveClass(/\bdark\b/);
    await expect(
      app.main.getByText("浏览器预览：外观只在当前会话生效，没有伪装成本地已保存。", { exact: true }),
    ).toBeVisible();
    await expect(app.main.getByText("本地设置已连接", { exact: true })).toHaveCount(0);
    await expect(app.main.getByText("已保存", { exact: true })).toHaveCount(0);
  });

  test("reduced-motion preference leaves page motion idle", async ({ page }) => {
    const app = new AppShellPage(page);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await app.open();

    const motion = await app.reducedMotionState();
    expect(motion.preferenceMatches).toBe(true);
    expect(motion.animationNames.length).toBeGreaterThan(0);
    expect(motion.animationNames.every((animationName) => animationName === "none")).toBe(true);
    expect(motion.runningCount).toBe(0);
  });

  test("navigation icons and page stages expose repeatable visible motion", async ({ page }) => {
    const app = new AppShellPage(page);
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await app.open();

    const analytics = app.navItem("分析");
    const icon = analytics.locator('[data-motion-id="analytics"]');
    const before = Number(await icon.getAttribute("data-motion-trigger"));
    await analytics.hover();
    const hoverMotion = await analytics.locator(".icon-bar-1").evaluate((element) => {
      const style = window.getComputedStyle(element);
      return { name: style.animationName, iterations: style.animationIterationCount };
    });
    expect(hoverMotion.name).toBe("none");
    expect(hoverMotion.iterations).toBe("1");
    await expect(icon).toHaveAttribute("data-motion-engaged", "true");
    await expect(analytics.locator(".icon-bar-1")).not.toHaveCSS("transform", "none");
    expect(Number(await icon.getAttribute("data-motion-trigger"))).toBe(before);

    await analytics.click();
    await app.heading("分析").waitFor();
    const pageStage = app.main.locator(".shell-page-stage");
    await expect(pageStage).toHaveAttribute("data-page-motion", "enter");
    await expect(pageStage).toHaveAttribute("data-reduced-motion", "false");
    expect(Number(await icon.getAttribute("data-motion-trigger"))).toBeGreaterThan(before);
  });

  test("keyboard Tab reaches a visibly focused primary navigation item", async ({ page, browserName }) => {
    const app = new AppShellPage(page);
    await app.open();

    const isMacWebKit = browserName === "webkit" && await page.evaluate(() => /Mac/.test(navigator.platform));
    const firstNavigation = await app.focusFirstPrimaryNavigation(isMacWebKit);
    await expect(firstNavigation).toBeFocused();
    expect(await app.hasVisibleFocus(firstNavigation)).toBe(true);
  });
});

async function expectProductCopy(scope: Locator, context: string) {
  const text = await scope.innerText();
  expect(text, `${context} must not expose internal test vocabulary`).not.toMatch(FORBIDDEN_PRODUCT_TEST_LANGUAGE);
  expect(text, `${context} must not use approval language`).not.toMatch(FORBIDDEN_APPROVAL_LANGUAGE);
  expect(text, `${context} must not use generic AI-style phrasing`).not.toMatch(FORBIDDEN_AI_STYLE_LANGUAGE);
}
