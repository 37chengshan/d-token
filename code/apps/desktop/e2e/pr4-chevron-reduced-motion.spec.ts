import { expect, test } from "@playwright/test";
import { AppShellPage, MAIN_PAGES } from "./pages/app-shell.page";

test.describe("PR-4 chevron reduced-motion verification", () => {
  test("statusbar chevron keeps no transform under reduced motion at 920x640", async ({ page }) => {
    const app = new AppShellPage(page);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await app.useCompactViewport();
    await app.open();

    const chevron = page.locator(".shell-statusbar-chevron").first();
    await expect(chevron).toBeVisible();
    await expect(chevron).toHaveCSS("transform", "none");

    // Expand the details panel; under reduced motion the chevron must not rotate.
    await page.getByRole("button", { name: "详情", exact: true }).click();
    await expect(chevron).toHaveCSS("transform", "none");
    await expect(page.getByText("数据来源", { exact: false }).first()).toBeVisible();
  });

  test("statusbar chevron rotates normally without reduced motion", async ({ page }) => {
    const app = new AppShellPage(page);
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await app.useCompactViewport();
    await app.open();

    const chevron = page.locator(".shell-statusbar-chevron").first();
    await expect(chevron).toBeVisible();
    const before = await chevron.evaluate((element) => window.getComputedStyle(element).transform);
    await page.getByRole("button", { name: "详情", exact: true }).click();
    const after = await chevron.evaluate((element) => window.getComputedStyle(element).transform);
    expect(after).not.toBe("none");
    expect(after).not.toBe(before);
  });

  test("all primary pages stay inside document width at 920x640 with reduced motion", async ({ page }) => {
    const app = new AppShellPage(page);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await app.useCompactViewport();
    await app.open();

    for (const target of MAIN_PAGES) {
      await app.navigateTo(target);
      const width = await app.documentWidth();
      expect(width.scrollWidth, `${target.heading} must not create horizontal document overflow`).toBeLessThanOrEqual(
        width.clientWidth,
      );
    }
  });
});
