import { expect, test, type Locator } from "@playwright/test";
import { AppShellPage, MAIN_PAGES } from "./pages/app-shell.page";

const FORBIDDEN_PRODUCT_TEST_LANGUAGE =
  /workload\s+benchmark|smoke\s+test|testing\s+center|test\s+center|fixture|mock|fake[-\s]?bin|测试中心|冒烟测试|基准测试|测试夹具|模拟(?:数据|服务)|假(?:数据|服务)/i;

test.describe("d-token desktop control plane", () => {
  test("all six primary pages are reachable from the shared shell", async ({ page }) => {
    const app = new AppShellPage(page);
    await app.open();

    for (const target of MAIN_PAGES) {
      await test.step(target.heading, async () => {
        await app.navigateTo(target);
        await expect(app.navItem(target.navLabel)).toHaveAttribute("aria-current", "page");
        await expect(app.heading(target.heading)).toBeVisible();
      });
    }
  });

  test("browser preview fails closed for native installation and change mutations", async ({ page }) => {
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
    await expect(app.main.getByText("浏览器预览", { exact: true }).first()).toBeVisible();
    await app.main.getByRole("button", { name: "安装并启动 d-token", exact: true }).click();
    await expect(
      app.main.getByText("当前是浏览器预览，真实安装事务需要在 Tauri 桌面窗口中运行。", {
        exact: true,
      }),
    ).toBeVisible();

    await app.navigateTo(MAIN_PAGES[4]);
    await expect(app.main.getByText("浏览器预览仅供查看，批准、拒绝和回退均不可执行。", { exact: true })).toBeVisible();
    await expectAllDisabled(app.main.getByRole("button", { name: "批准", exact: true }));
    await expectAllDisabled(app.main.getByRole("button", { name: "拒绝", exact: true }));

    await app.main.getByRole("tab", { name: "备份与回滚", exact: true }).click();
    await expectAllDisabled(app.main.getByRole("button", { name: "回滚到此处", exact: true }));
    expect(unexpectedExternalRequests).toEqual([]);
  });

  test("unavailable native and destructive settings remain disabled in preview", async ({ page }) => {
    const app = new AppShellPage(page);
    await app.open();
    await app.navigateTo(MAIN_PAGES[5]);
    await expect(app.main.getByText("浏览器预览", { exact: true }).first()).toBeVisible();

    await expect(app.main.getByRole("button", { name: /^(启动服务|停止服务)$/ })).toHaveCount(0);
    for (const label of [
      "自动启动 d-token 服务",
      "诊断数据上传",
      "智能学习全面扫描",
      "自动检查更新",
    ]) {
      await expect(app.main.getByRole("switch", { name: label, exact: true })).toBeDisabled();
    }
    await expect(app.main.getByRole("button", { name: "深度扫描", exact: true })).toBeDisabled();
    await expect(app.main.getByRole("button", { name: "管理范围", exact: true })).toBeDisabled();
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

    await app.navigateTo(MAIN_PAGES[3]);
    for (const tab of ["健康检查", "Runtime", "维护"]) {
      await app.main.getByRole("tab", { name: tab, exact: true }).click();
      await expectProductCopy(app.main, `诊断 / ${tab}`);
    }
  });
});

async function expectAllDisabled(locator: Locator) {
  const count = await locator.count();
  expect(count).toBeGreaterThan(0);
  for (let index = 0; index < count; index += 1) {
    await expect(locator.nth(index)).toBeDisabled();
  }
}

async function expectProductCopy(scope: Locator, context: string) {
  const text = await scope.innerText();
  expect(text, `${context} must not expose internal test vocabulary`).not.toMatch(FORBIDDEN_PRODUCT_TEST_LANGUAGE);
}
