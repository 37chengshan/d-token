import { chromium } from "playwright";
import path from "path";
import fs from "fs";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const desktopRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(desktopRoot, "../..");
const showcaseDir = path.resolve(desktopRoot, "showcase");

const exportDirs = [
  path.resolve(showcaseDir, "exports"),
  path.resolve(repoRoot, "support/artifacts/figma-export"),
];

for (const dir of exportDirs) {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
}

const PAGES = [
  { id: "overview", name: "01-overview", title: "总体概览 (Overview)" },
  { id: "install", name: "02-install", title: "安装与连接 (Install & Connect)" },
  { id: "codebase-memory", name: "03-codebase-memory", title: "代码图谱 (Code Graph)" },
  { id: "analytics", name: "04-analytics", title: "分析 (Analytics)" },
  { id: "visual", name: "05-visual", title: "视觉压缩 (Visual Compression)" },
  { id: "doctor", name: "06-doctor", title: "Doctor (Diagnostics)" },
  { id: "changes", name: "07-changes", title: "变更与回退 (Changes & Rollback)" },
  { id: "settings", name: "08-settings", title: "系统设置 (Settings)" },
];

const MODALS = [
  { id: "modal-onboarding", name: "09-onboarding", title: "首次引导向导 (Onboarding Wizard)" },
  { id: "modal-tray", name: "10-tray-panel", title: "系统托盘面板 (System Tray Panel)" },
];

async function captureAll() {
  console.log("Launching Chromium for Figma export capture...");
  const browser = await chromium.launch({
    headless: true,
  });

  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 2, // 2x for crystal-clear retina rendering in Figma
  });

  const page = await context.newPage();
  const indexUrl = "file://" + path.resolve(showcaseDir, "index.html");

  console.log(`Loading showcase from: ${indexUrl}`);
  await page.goto(indexUrl, { waitUntil: "networkidle" });

  // CSS injection to cleanly hide dev-bar and position viewport at top 0
  await page.addStyleTag({
    content: `
      .showcase-dev-bar { display: none !important; }
      .showcase-viewport { top: 0 !important; }
    `,
  });

  const modes = [
    { name: "light", isDark: false },
    { name: "dark", isDark: true },
  ];

  for (const mode of modes) {
    console.log(`\n--- Capturing mode: ${mode.name} ---`);
    await page.evaluate((isDark) => {
      if (isDark) {
        document.documentElement.classList.add("dark");
      } else {
        document.documentElement.classList.remove("dark");
      }
    }, mode.isDark);

    // 1. Capture 8 Main Pages
    for (const p of PAGES) {
      console.log(`Capturing page [${p.id}] in ${mode.name} mode...`);
      await page.evaluate((pageId) => {
        // Close any modals
        document.querySelectorAll(".modal-backdrop").forEach((m) => m.classList.remove("active"));
        if (typeof window.dtokenShowcase !== "undefined" && window.dtokenShowcase.setPage) {
          window.dtokenShowcase.setPage(pageId);
        } else {
          document.querySelectorAll(".page-view").forEach((el) => {
            el.classList.toggle("active", el.id === `page-${pageId}`);
          });
        }
      }, p.id);

      await page.waitForTimeout(300);

      const fileName = `${p.name}-${mode.name}.png`;
      for (const outDir of exportDirs) {
        const dest = path.resolve(outDir, fileName);
        await page.screenshot({ path: dest });
      }
      console.log(` Saved: ${fileName}`);
    }

    // 2. Capture Modals
    for (const m of MODALS) {
      console.log(`Capturing modal [${m.id}] in ${mode.name} mode...`);
      await page.evaluate((modalId) => {
        document.querySelectorAll(".modal-backdrop").forEach((backdrop) => backdrop.classList.remove("active"));
        const modal = document.getElementById(modalId);
        if (modal) modal.classList.add("active");
      }, m.id);

      await page.waitForTimeout(300);

      const fileName = `${m.name}-${mode.name}.png`;
      for (const outDir of exportDirs) {
        const dest = path.resolve(outDir, fileName);
        await page.screenshot({ path: dest });
      }
      console.log(` Saved: ${fileName}`);

      // Close modal
      await page.evaluate(() => {
        document.querySelectorAll(".modal-backdrop").forEach((backdrop) => backdrop.classList.remove("active"));
      });
    }
  }

  await browser.close();
  console.log("\nAll screenshots successfully captured!");

  // Generate self-contained standalone HTML pages for Figma html.to.design import
  console.log("\nGenerating standalone HTML bundles for Figma html.to.design vector import...");
  generateFigmaHtmlBundles();

  // Generate Figma Import Guide
  generateFigmaImportGuide();
}

function generateFigmaHtmlBundles() {
  const appBundleCss = fs.readFileSync(path.resolve(showcaseDir, "app-bundle.css"), "utf-8");
  const showcaseCss = fs.readFileSync(path.resolve(showcaseDir, "showcase.css"), "utf-8");

  // Read individual page html fragments
  const pagesDir = path.resolve(showcaseDir, "pages");
  const pageFiles = fs.readdirSync(pagesDir).filter((f) => f.endsWith(".html"));

  for (const pageFile of pageFiles) {
    const pageContent = fs.readFileSync(path.resolve(pagesDir, pageFile), "utf-8");
    const pageBase = pageFile.replace(".html", "");

    // Create standalone HTML in Light and Dark modes
    for (const theme of ["light", "dark"]) {
      const isDark = theme === "dark";
      const standaloneHtml = `<!DOCTYPE html>
<html lang="zh-CN" class="${isDark ? "dark" : ""}">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=1280, initial-scale=1.0" />
  <title>d-token - ${pageBase} (${theme})</title>
  <style>
${appBundleCss}
${showcaseCss}
  body {
    background-color: var(--background);
    color: var(--foreground);
    padding: 24px;
    box-sizing: border-box;
    width: 1280px;
    min-height: 800px;
  }
  </style>
</head>
<body class="bg-background text-foreground antialiased selection:bg-orange-500/20">
  <div class="max-w-[1200px] mx-auto">
${pageContent}
  </div>
</body>
</html>`;

      const outName = `${pageBase}-${theme}.html`;
      for (const outDir of exportDirs) {
        fs.writeFileSync(path.resolve(outDir, outName), standaloneHtml, "utf-8");
      }
    }
  }

  console.log("Standalone HTML files generated in both light and dark themes.");
}

function generateFigmaImportGuide() {
  const guideContent = `# d-token 完整应用页面 Figma 导入指南

本指南指导您将 \`d-token\` 现存应用的全部 10 大页面（8 大主页面 + 首次引导 + 系统托盘浮窗）完整、无损地导入到 Figma 画布中。

- **关联 Figma 文件地址**：
  \`https://www.figma.com/design/oIccsjtjaHJg86RlrGDY4P/d-token-%C2%B7-%E5%BD%93%E5%89%8D%E7%95%8C%E9%9D%A2%E4%B8%8E%E4%BA%94%E9%A1%B5%E9%87%8D%E6%9E%84-%C2%B7-2026-09-19?node-id=12-6&m=dev\`

---

## 方案一：1:1 像素级高清画板拖入（最快捷，推荐用于视觉参照与走查）

在导出目录（\`support/artifacts/figma-export/\` 或 \`code/apps/desktop/showcase/exports/\`）中，已包含由 Playwright 以 Retina 2x 自动化截取的 20 张高清画板（1280x800）：

### 浅色模式（Warm Minimalist，底色与 Figma 节点 12:6/12:2 严格一致 #e9e5dc）：
1. \`01-overview-light.png\`：总体概览（4 组 KPI、折线趋势图、活动热力图、上游代理状态）
2. \`02-install-light.png\`：安装与连接工作台（一键事务配置、智能学习、高级端点）
3. \`03-codebase-memory-light.png\`：代码图谱（拓扑图示、实体关系树、Graph Trace 控制台）
4. \`04-analytics-light.png\`：分析（削减走势、模型计费效益表、延迟网络分解）
5. \`05-visual-light.png\`：视觉压缩 (Beta)（Optical 引擎状态、图像对比检视器）
6. \`06-doctor-light.png\`：Doctor（健康自检仪表盘、诊断项清单、诊断终端输出）
7. \`07-changes-light.png\`：变更与回退（还原点清单、前后配置 Diff 视窗、回退确认）
8. \`08-settings-light.png\`：系统设置（外观主题、强调色、端口安全、凭据库）
9. \`09-onboarding-light.png\`：首次引导向导（环境检测、Agent 扫描、一键进入）
10. \`10-tray-panel-light.png\`：系统托盘面板（360px 紧凑悬浮窗、服务启停、今日节省）

### 深色模式：
- \`01-overview-dark.png\` 至 \`10-tray-panel-dark.png\`（对应深色青灰/碳黑质感）

**导入步骤**：
直接将上述 PNG 文件全选，拖入到您的 Figma 画布中，排列在 \`12:2\` 画板旁即可直接作为 1:1 精确对照底图！

---

## 方案二：通过 Figma "html.to.design" 插件导入 100% 全矢量可编辑图层（推荐用于重构与组件提取）

如果您希望在 Figma 中获得**可编辑的文字图层、矢量矩形、Auto Layout 布局结构、CSS 变量色彩**：

### 步骤：
1. **在 Figma 中启动插件**：
   在 Figma 菜单中选择 \`Plugins\` -> 搜索并打开 \`html.to.design\`。
2. **选择导入方式**：
   - **方式 A (直接上传文件)**：
     点击插件界面的 \`Import HTML file\`，选择本导出目录下的 \`*-light.html\` 或 \`*-dark.html\` 单页文件，插件将自动解析 DOM 并生成全矢量 Figma Frame。
   - **方式 B (本地预览实时导入)**：
     在终端启动本地静态服务：
     \`\`\`bash
     cd code/apps/desktop/showcase
     npx serve .
     \`\`\`
     然后在 \`html.to.design\` 插件中输入 \`http://localhost:3000\`，输入视口宽度 \`1280\`，高度 \`800\`，点击 \`Import\` 即可将当前页面一键转换为 Figma 全矢量画板！
`;

  for (const outDir of exportDirs) {
    fs.writeFileSync(path.resolve(outDir, "FIGMA_IMPORT_GUIDE.md"), guideContent, "utf-8");
  }
  console.log("FIGMA_IMPORT_GUIDE.md written to export directories.");
}

captureAll().catch((err) => {
  console.error("Capture failed:", err);
  process.exit(1);
});
