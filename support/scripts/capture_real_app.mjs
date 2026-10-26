import { chromium } from "playwright";
import { preview } from "vite";
import path from "path";
import fs from "fs";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const desktopRoot = path.resolve(__dirname, "../../code/apps/desktop");
const outDir = path.resolve(__dirname, "../artifacts/figma-export");

if (!fs.existsSync(outDir)) {
  fs.mkdirSync(outDir, { recursive: true });
}

const PAGES = [
  { id: "overview", name: "01-overview", navLabel: "总体概览", heading: "总体概览" },
  { id: "install", name: "02-install", navLabel: "安装与连接", heading: "安装与连接" },
  { id: "codebase-memory", name: "03-codebase-memory", navLabel: "代码图谱", heading: "代码图谱（实验性）" },
  { id: "analytics", name: "04-analytics", navLabel: "分析", heading: "分析" },
  { id: "visual", name: "05-visual", navLabel: "视觉压缩 Beta", heading: "视觉压缩" },
  { id: "doctor", name: "06-doctor", navLabel: "Doctor", heading: "Doctor" },
  { id: "changes", name: "07-changes", navLabel: "变更与回退", heading: "变更与回退" },
  { id: "settings", name: "08-settings", navLabel: "设置", heading: "系统设置" },
];

async function main() {
  console.log("Starting Vite preview server on 127.0.0.1:5188...");
  const server = await preview({
    root: desktopRoot,
    preview: {
      port: 5188,
      host: "127.0.0.1",
      strictPort: true,
    },
  });

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 2,
    locale: "zh-CN",
  });

  const page = await context.newPage();
  await page.goto("http://127.0.0.1:5188/");
  await page.waitForLoadState("networkidle");

  // Ensure Light Mode first
  console.log("Setting light mode...");
  const lightRadio = page.getByRole("radio", { name: "浅色", exact: true });
  if (await lightRadio.isVisible()) {
    await lightRadio.click();
    await page.waitForTimeout(300);
  }

  // Capture Light Mode pages
  for (const item of PAGES) {
    console.log(`Capturing Light: ${item.name} (${item.navLabel})...`);
    const btn = page.getByRole("button", { name: item.navLabel, exact: true });
    await btn.click();
    await page.waitForTimeout(500);
    const targetFile = path.join(outDir, `${item.name}-light.png`);
    await page.screenshot({ path: targetFile });
    console.log(`Saved: ${targetFile}`);
  }

  // Capture Onboarding Light
  const onboardingBtn = page.getByRole("button", { name: /新手引导向导|配置/ });
  if (await onboardingBtn.first().isVisible()) {
    await onboardingBtn.first().click();
    await page.waitForTimeout(500);
    const targetFile = path.join(outDir, "09-onboarding-light.png");
    await page.screenshot({ path: targetFile });
    console.log(`Saved: ${targetFile}`);
    // Close modal if possible
    const closeBtn = page.getByRole("button", { name: /稍后|关闭|完成/ });
    if (await closeBtn.first().isVisible()) {
      await closeBtn.first().click();
      await page.waitForTimeout(300);
    }
  }

  // Switch to Dark Mode
  console.log("Setting dark mode...");
  const darkRadio = page.getByRole("radio", { name: "深色", exact: true });
  if (await darkRadio.isVisible()) {
    await darkRadio.click();
    await page.waitForTimeout(300);
  }

  // Capture Dark Mode pages
  for (const item of PAGES) {
    console.log(`Capturing Dark: ${item.name} (${item.navLabel})...`);
    const btn = page.getByRole("button", { name: item.navLabel, exact: true });
    await btn.click();
    await page.waitForTimeout(500);
    const targetFile = path.join(outDir, `${item.name}-dark.png`);
    await page.screenshot({ path: targetFile });
    console.log(`Saved: ${targetFile}`);
  }

  await browser.close();
  server.httpServer.close();
  console.log("All real app pages captured successfully!");
}

main().catch((err) => {
  console.error("Capture failed:", err);
  process.exit(1);
});
