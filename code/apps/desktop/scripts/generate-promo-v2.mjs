import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { chromium } from "@playwright/test";
import { createServer } from "vite";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(desktopRoot, "../../..");
const outputRoot = path.join(repoRoot, "docs/media/promo/v2");
const screenshotsRoot = path.join(outputRoot, "screenshots");
const iconPath = path.join(desktopRoot, "src/assets/dtoken-cat-icon.png");

const WIDTH = 1600;
const HEIGHT = 900;
const APP_VIEWPORT = { width: 1512, height: 992 };
const FONT_SANS = `ui-sans-serif, -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', 'PingFang SC', 'Microsoft YaHei UI', sans-serif`;
const FONT_SERIF = `ui-serif, 'Iowan Old Style', 'Palatino Linotype', 'Songti SC', STSong, serif`;
const FONT_MONO = `'SFMono-Regular', Consolas, 'Liberation Mono', monospace`;

const palette = {
  ink: "#211d1a",
  muted: "#746a62",
  paper: "#f4efe7",
  paper2: "#ece5da",
  panel: "#fffdf9",
  line: "#d9d0c4",
  accent: "#d66f4d",
  accentSoft: "#f4d7ca",
  success: "#3f7d5d",
  warning: "#a36e22",
  dark: "#211e1b",
  dark2: "#2b2723",
  dark3: "#332f2b",
  darkLine: "#4b443e",
  lightOnDark: "#fff9f2",
  mutedOnDark: "#c9bfb5",
};

const copy = {
  zh: {
    localeDir: "zh-CN",
    localeSuffix: "zh-CN",
    hero: {
      eyebrow: "桌面控制平面",
      title: ["一个桌面入口，", "掌控 AI Coding 工作流。"],
      lede: [
        "统一连接 Agent、管理 Provider 与 Route、观察请求与 Token，",
        "并用 Doctor、变更记录与回退证据处理本地运行问题。",
      ],
      pills: ["显式连接路径", "运行证据", "Doctor 诊断", "可验证回退"],
      status: "本地优先 · Rust Runtime",
      footer: "为更安全、可观测、可恢复的 AI Coding 工作流而构建。",
      label: "真实最新界面",
    },
    theme: {
      eyebrow: "统一外观系统",
      title: ["浅色、深色与", "可配置主题色。"],
      lede: [
        "支持浅色、深色与跟随系统模式，并可分别配置界面强调色",
        "和分析图表主色；语言、页面结构与操作语义保持一致。",
      ],
      light: "浅色主题",
      custom: "主题与颜色配置",
      dark: "深色主题",
      modes: ["浅色", "深色", "跟随系统"],
      facts: [
        ["5 组界面强调色", "蓝、绿、橙、紫、灰"],
        ["6 组图表主色", "绿、蓝、琥珀、玫红、紫、橙"],
        ["同一页面语义", "主题切换不改变工作流"],
      ],
      index: "02 / 主题",
    },
    visual: {
      eyebrow: "视觉压缩 · Beta",
      title: ["先用 Shadow 预览，", "再决定是否调整策略。"],
      lede: [
        "视觉压缩页展示状态、能力、生命周期、事件与 Shadow Plan；",
        "产品路径强制 Active 关闭，默认不改变任何真实请求。",
      ],
      badge: "SHADOW 默认 · ACTIVE 关闭",
      pipelineTitle: "受控的视觉压缩配置流程",
      steps: [
        ["01", "读取能力边界", "读取 status、capabilities、lifecycle 与 events，不渲染原始请求体。"],
        ["02", "生成 Shadow 计划", "规划和估算仅用于观察；估算节省不等于真实账单。"],
        ["03", "按 Route 预览 Diff", "高级配置限定到明确 Route，并展示变更与重启影响。"],
        ["04", "确认写入并保留回退", "通过 ConfigPatcher 写盘与重启；allow_active 始终强制为 false。"],
      ],
      resultTitle: "真实请求保持不变",
      resultBody: "Shadow-only · Redacted metadata · Reversible config",
      screenshotLabel: "最新视觉压缩界面",
      pills: ["Route-scoped", "Shadow-only", "Redacted", "Diff & Rollback"],
      index: "03 / 视觉压缩",
    },
    graph: {
      eyebrow: "代码图谱 · 实验性",
      title: ["固定版本导入，", "显式连接与只读图谱。"],
      lede: [
        "用户自行导入 Codebase Memory MCP v0.9.1-rc.1，完成平台、架构与",
        "SHA-256 校验后，再按明确项目范围连接 Agent、建立索引并打开只读图谱。",
      ],
      screenTitle: "代码图谱工作台",
      screenMeta: "真实本地状态",
      flowTitle: "受控启用流程",
      flowMeta: "实验性能力",
      steps: [
        ["01", "导入固定组件", "打开官方下载页并解压到默认或自定义目录。"],
        ["02", "完成完整校验", "平台、架构、版本与 pinned SHA-256 分别确认。"],
        ["03", "扫描并选择 Agent", "批量操作必须显示 Agent、阶段与 current / total 进度。"],
        ["04", "预览、配置、验证", "多文件事务聚合为一个可恢复回退点。"],
        ["05", "建立索引并打开图谱", "使用显式项目范围与 d-token-owned 只读图谱。"],
      ],
      footerSteps: ["导入", "校验", "连接", "索引", "只读图谱"],
      index: "04 / 代码图谱",
    },
  },
  en: {
    localeDir: "en",
    localeSuffix: "en",
    hero: {
      eyebrow: "DESKTOP CONTROL PLANE",
      title: ["One desktop control plane", "for AI coding workflows."],
      lede: [
        "Connect agents, manage providers and routes, inspect requests and tokens,",
        "then diagnose and recover local operations with evidence.",
      ],
      pills: ["Explicit routes", "Runtime evidence", "Doctor diagnostics", "Verified rollback"],
      status: "LOCAL-FIRST · RUST RUNTIME",
      footer: "Built for a safer, observable, and recoverable AI coding workflow.",
      label: "LATEST REAL UI",
    },
    theme: {
      eyebrow: "UNIFIED APPEARANCE SYSTEM",
      title: ["Light, dark, system-aware—", "with configurable accents."],
      lede: [
        "Choose light, dark, or system mode, then configure UI accents and",
        "analytics colors independently without changing page or workflow semantics.",
      ],
      light: "LIGHT THEME",
      custom: "THEME & COLOR CONTROLS",
      dark: "DARK THEME",
      modes: ["Light", "Dark", "System"],
      facts: [
        ["5 UI accents", "Blue, green, orange, purple, slate"],
        ["6 chart accents", "Green, blue, amber, rose, violet, orange"],
        ["Stable semantics", "Theme changes never alter the workflow"],
      ],
      index: "02 / THEME",
    },
    visual: {
      eyebrow: "VISUAL COMPRESSION · BETA",
      title: ["Preview in Shadow first.", "Change policy only with evidence."],
      lede: [
        "The Visual Compression page exposes status, capabilities, lifecycle, events,",
        "and Shadow plans. The product path forces Active off and leaves live requests unchanged.",
      ],
      badge: "SHADOW DEFAULT · ACTIVE OFF",
      pipelineTitle: "CONTROLLED VISUAL CONFIGURATION FLOW",
      steps: [
        ["01", "Read capability boundaries", "Read status, capabilities, lifecycle, and events without rendering raw request bodies."],
        ["02", "Generate a Shadow plan", "Plans and estimates are observational; estimated savings are not billing."],
        ["03", "Preview route-scoped diffs", "Advanced policy stays bound to an explicit route and shows restart impact."],
        ["04", "Confirm writes with rollback", "ConfigPatcher writes and restarts safely while allow_active remains forced false."],
      ],
      resultTitle: "LIVE REQUESTS STAY UNCHANGED",
      resultBody: "Shadow-only · Redacted metadata · Reversible config",
      screenshotLabel: "LATEST VISUAL COMPRESSION UI",
      pills: ["Route-scoped", "Shadow-only", "Redacted", "Diff & rollback"],
      index: "03 / VISUAL",
    },
    graph: {
      eyebrow: "CODE GRAPH · EXPERIMENTAL",
      title: ["Pinned import, explicit setup,", "and a read-only graph."],
      lede: [
        "Import Codebase Memory MCP v0.9.1-rc.1 yourself, verify platform, architecture,",
        "and pinned SHA-256, then connect agents, build indexes, and open the d-token read-only graph.",
      ],
      screenTitle: "CODE GRAPH WORKBENCH",
      screenMeta: "REAL LOCAL STATE",
      flowTitle: "CONTROLLED ENABLEMENT FLOW",
      flowMeta: "EXPERIMENTAL CAPABILITY",
      steps: [
        ["01", "Import the pinned bundle", "Open the official download page and extract to the default or chosen directory."],
        ["02", "Complete full verification", "Confirm platform, architecture, version, and pinned SHA-256 independently."],
        ["03", "Scan and select agents", "Batch work reports agent, phase, and current / total progress."],
        ["04", "Preview, configure, verify", "Multi-file work is aggregated into one recoverable rollback point."],
        ["05", "Index and open the graph", "Use an explicit project scope and the d-token-owned read-only graph."],
      ],
      footerSteps: ["Import", "Verify", "Connect", "Index", "Read-only graph"],
      index: "04 / GRAPH",
    },
  },
};

const captureSpecs = [
  ["zh", "overview", "light"],
  ["zh", "overview", "dark"],
  ["zh", "visual", "light"],
  ["zh", "codebase-memory", "light"],
  ["en", "overview", "light"],
  ["en", "overview", "dark"],
  ["en", "visual", "light"],
  ["en", "codebase-memory", "light"],
];

function esc(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function text(x, y, value, {
  fill = palette.ink,
  size = 16,
  weight = 500,
  family = FONT_SANS,
  anchor = "start",
  spacing = 0,
  opacity = 1,
} = {}) {
  return `<text x="${x}" y="${y}" fill="${fill}" font-family="${family}" font-size="${size}" font-weight="${weight}" text-anchor="${anchor}" letter-spacing="${spacing}" opacity="${opacity}">${esc(value)}</text>`;
}

function textLines(lines, {
  x,
  y,
  size,
  lineHeight,
  fill,
  weight = 500,
  family = FONT_SANS,
  spacing = 0,
} = {}) {
  return lines.map((line, index) => text(x, y + index * lineHeight, line, { fill, size, weight, family, spacing })).join("\n");
}

function roundedImage({ id, x, y, width, height, dataUri, radius = 20, rotate = 0, shadow = true, fit = "meet" }) {
  const transform = rotate ? ` transform="rotate(${rotate} ${x + width / 2} ${y + height / 2})"` : "";
  return `<g${transform}${shadow ? ` filter="url(#shadow-${id})"` : ""}>
    <rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="#f8f4ed" stroke="rgba(74,61,50,.28)"/>
    <image x="${x}" y="${y}" width="${width}" height="${height}" preserveAspectRatio="xMidYMid ${fit}" clip-path="url(#clip-${id})" href="${dataUri}"/>
  </g>`;
}

function defs(id, clipRects) {
  return `<defs>
    <filter id="shadow-${id}" x="-25%" y="-25%" width="150%" height="170%"><feDropShadow dx="0" dy="24" stdDeviation="28" flood-color="#241b15" flood-opacity="0.24"/></filter>
    ${clipRects.map((r) => `<clipPath id="clip-${r.id}"><rect x="${r.x}" y="${r.y}" width="${r.width}" height="${r.height}" rx="${r.radius ?? 20}"/></clipPath>`).join("\n")}
  </defs>`;
}

function brand(icon, dark = false, x = 72, y = 62) {
  const ink = dark ? palette.lightOnDark : palette.ink;
  const muted = dark ? palette.mutedOnDark : palette.muted;
  return `<g transform="translate(${x} ${y})">
    <image x="0" y="0" width="46" height="46" href="${icon}"/>
    ${text(58, 23, "d-token", { fill: ink, size: 24, weight: 760, spacing: -0.5 })}
    ${text(58, 41, "AI CODING CONTROL PLANE", { fill: muted, size: 10.5, weight: 700, spacing: 1.8 })}
  </g>`;
}

function eyebrow(value, x, y, dark = false) {
  const color = dark ? "#e3b6a6" : palette.accent;
  return `<g><rect x="${x}" y="${y - 9}" width="26" height="3" rx="2" fill="${palette.accent}"/>${text(x + 36, y, value, { fill: color, size: 13, weight: 740, spacing: 1.6 })}</g>`;
}

function pillRow(items, x, y, dark = false, maxWidth = 570) {
  let cursorX = x;
  let cursorY = y;
  const lineHeight = 46;
  const gap = 10;
  return items.map((item) => {
    const width = Math.max(126, Math.min(260, 44 + item.length * (/[\u3400-\u9fff]/.test(item) ? 15.2 : 8.2)));
    if (cursorX + width > x + maxWidth) {
      cursorX = x;
      cursorY += lineHeight;
    }
    const result = `<g transform="translate(${cursorX} ${cursorY})">
      <rect width="${width}" height="36" rx="18" fill="${dark ? "#292521" : "rgba(255,255,255,.64)"}" stroke="${dark ? palette.darkLine : palette.line}"/>
      <circle cx="17" cy="18" r="4" fill="${palette.accent}"/>
      ${text(29, 23, item, { fill: dark ? "#f8eee5" : palette.ink, size: 13, weight: 670 })}
    </g>`;
    cursorX += width + gap;
    return result;
  }).join("\n");
}

async function captureLatestUi() {
  await mkdir(screenshotsRoot, { recursive: true });
  const server = await createServer({
    root: desktopRoot,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 5193, strictPort: true },
  });
  await server.listen();
  const baseUrl = server.resolvedUrls?.local?.[0] ?? "http://127.0.0.1:5193/";
  const browser = await chromium.launch({ headless: true });

  try {
    for (const [lang, pageId, theme] of captureSpecs) {
      const context = await browser.newContext({
        viewport: APP_VIEWPORT,
        deviceScaleFactor: 1,
        colorScheme: theme,
        reducedMotion: "reduce",
      });
      await context.addInitScript(({ langValue, pageValue, themeValue }) => {
        const appearance = {
          theme: themeValue,
          accent: "orange",
          chartAccent: "orange",
          glass: false,
          lang: langValue,
        };
        window.localStorage.setItem("d-token.appearance.v2", JSON.stringify(appearance));
        window.localStorage.setItem("d-token.theme", themeValue);
        window.localStorage.setItem("d-token.shell.sidebar-collapsed", "0");
        window.sessionStorage.setItem("d-token.shell.page", pageValue);
      }, { langValue: lang, pageValue: pageId, themeValue: theme });

      const page = await context.newPage();
      await page.goto(baseUrl, { waitUntil: "networkidle" });
      await page.locator(".app-shell").waitFor({ state: "visible", timeout: 20000 });
      await page.evaluate(() => document.fonts.ready);
      await page.waitForTimeout(1200);
      const outputPath = path.join(screenshotsRoot, `${lang}-${pageId}-${theme}.png`);
      await page.screenshot({ path: outputPath, fullPage: false, animations: "disabled" });
      console.log(`captured ${path.relative(repoRoot, outputPath)}`);
      await context.close();
    }
  } finally {
    await browser.close();
    await server.close();
  }
}

async function dataUri(filePath, mime = "image/png") {
  const buffer = await readFile(filePath);
  return `data:${mime};base64,${buffer.toString("base64")}`;
}

function svgFrame({ title, desc, body, defsContent = "" }) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}" role="img" aria-labelledby="title desc">
  <title id="title">${esc(title)}</title>
  <desc id="desc">${esc(desc)}</desc>
  ${defsContent}
  ${body}
</svg>`;
}

function renderHero(lang, assets) {
  const c = copy[lang].hero;
  const screenshot = assets[`${lang}-overview-light`];
  const clip = { id: `hero-${lang}`, x: 794, y: 150, width: 910, height: 600, radius: 20 };
  const body = `
    <rect width="1600" height="900" fill="${palette.dark}"/>
    <circle cx="1450" cy="80" r="280" fill="#d66f4d" opacity="0.05"/>
    ${brand(assets.icon, true)}
    <g transform="translate(1324 72)"><rect width="204" height="34" rx="17" fill="#292521" stroke="${palette.darkLine}"/>${text(102, 22, c.status, { fill: palette.mutedOnDark, size: lang === "zh" ? 10.5 : 10.2, weight: 700, family: FONT_MONO, anchor: "middle", spacing: 0.7 })}</g>
    ${eyebrow(c.eyebrow, 74, 209, true)}
    ${textLines(c.title, { x: 74, y: 305, size: lang === "zh" ? 65 : 61, lineHeight: 69, fill: palette.lightOnDark, weight: 560, family: FONT_SERIF, spacing: -1.6 })}
    ${textLines(c.lede, { x: 74, y: lang === "zh" ? 500 : 490, size: lang === "zh" ? 20 : 19.5, lineHeight: 34, fill: palette.mutedOnDark, weight: 440 })}
    ${pillRow(c.pills, 74, 620, true, 590)}
    ${roundedImage({ id: clip.id, x: clip.x, y: clip.y, width: clip.width, height: clip.height, dataUri: screenshot, radius: 20, rotate: -2.4, fit: "meet" })}
    <g transform="translate(1410 701) rotate(-2.4)"><rect width="114" height="28" rx="8" fill="rgba(33,29,26,.86)" stroke="rgba(255,255,255,.14)"/>${text(57, 18, c.label, { fill: "#fff8f1", size: lang === "zh" ? 9.5 : 9, weight: 720, family: FONT_MONO, anchor: "middle", spacing: 0.5 })}</g>
    ${text(74, 851, c.footer, { fill: "#9e9389", size: 13, weight: 500, spacing: 0.25 })}
    <rect x="0" y="889" width="1600" height="11" fill="${palette.accent}"/>
    ${text(1528, 862, "01 / PRODUCT", { fill: "#a89d93", size: 12.5, weight: 500, family: FONT_MONO, anchor: "end", spacing: 1 })}
  `;
  return svgFrame({ title: c.title.join(" "), desc: c.lede.join(" "), body, defsContent: defs(`hero-${lang}`, [clip]) });
}

function themeScreenshotCard({ id, x, y, width, height, title, image, darkLabel = false }) {
  return `<g>
    ${text(x, y - 16, title, { fill: palette.ink, size: 13, weight: 730, family: FONT_MONO, spacing: 1.1 })}
    ${roundedImage({ id, x, y, width, height, dataUri: image, radius: 15, fit: "meet" })}
    ${darkLabel ? `<g transform="translate(${x + width - 76} ${y + 16})"><rect width="60" height="24" rx="12" fill="#292521" opacity=".9"/>${text(30, 16, "DARK", { fill: "#fff9f2", size: 9, weight: 720, family: FONT_MONO, anchor: "middle", spacing: 1 })}</g>` : ""}
  </g>`;
}

function renderTheme(lang, assets) {
  const c = copy[lang].theme;
  const light = assets[`${lang}-overview-light`];
  const dark = assets[`${lang}-overview-dark`];
  const clips = [
    { id: `theme-light-${lang}`, x: 96, y: 398, width: 430, height: 282, radius: 15 },
    { id: `theme-dark-${lang}`, x: 1074, y: 398, width: 430, height: 282, radius: 15 },
  ];
  const uiAccents = ["#4f6f9b", "#3b7a57", "#c7652a", "#80639b", "#657078"];
  const chartAccents = ["#10b981", "#38bdf8", "#f59e0b", "#fb7185", "#a78bfa", "#f97316"];
  const modeButtons = c.modes.map((mode, index) => `<g transform="translate(${568 + index * 102} 466)"><rect width="92" height="38" rx="12" fill="${index === 0 ? palette.accentSoft : "#f7f2eb"}" stroke="${index === 0 ? palette.accent : palette.line}"/>${text(46, 24, mode, { fill: index === 0 ? "#8b432d" : palette.ink, size: 12, weight: 680, anchor: "middle" })}</g>`).join("\n");
  const uiSwatches = uiAccents.map((color, index) => `<circle cx="${588 + index * 44}" cy="556" r="14" fill="${color}"${index === 2 ? ` stroke="#fff" stroke-width="4"` : ""}/>`).join("\n");
  const chartSwatches = chartAccents.map((color, index) => `<circle cx="${588 + index * 38}" cy="628" r="11" fill="${color}"/>`).join("\n");
  const facts = c.facts.map(([title, body], index) => `<g transform="translate(${100 + index * 492} 749)">${text(0, 0, title, { fill: palette.ink, size: 14, weight: 730 })}${text(0, 23, body, { fill: palette.muted, size: lang === "zh" ? 12 : 11.3, weight: 500 })}</g>`).join("\n");
  const body = `
    <rect width="1600" height="900" fill="${palette.paper}"/>
    <circle cx="1520" cy="20" r="230" fill="#d66f4d" opacity="0.045"/>
    ${eyebrow(c.eyebrow, 68, 72)}
    ${textLines(c.title, { x: 68, y: 149, size: lang === "zh" ? 60 : 56, lineHeight: 61, fill: palette.ink, weight: 560, family: FONT_SERIF, spacing: -1.5 })}
    ${textLines(c.lede, { x: 68, y: 276, size: lang === "zh" ? 20 : 18.5, lineHeight: 32, fill: palette.muted, weight: 450 })}
    ${brand(assets.icon, false, 1295, 55)}
    <rect x="68" y="352" width="1464" height="476" rx="24" fill="${palette.panel}" stroke="${palette.line}" filter="url(#shadow-theme-panel-${lang})"/>
    ${themeScreenshotCard({ id: clips[0].id, x: 96, y: 398, width: 430, height: 282, title: c.light, image: light })}
    ${text(550, 382, c.custom, { fill: palette.ink, size: 13, weight: 730, family: FONT_MONO, spacing: 1.1 })}
    <rect x="550" y="398" width="500" height="282" rx="18" fill="#fbf8f3" stroke="${palette.line}"/>
    ${text(578, 438, lang === "zh" ? "主题模式" : "THEME MODE", { fill: palette.muted, size: 11, weight: 720, family: FONT_MONO, spacing: 1.1 })}
    ${modeButtons}
    ${text(578, 534, lang === "zh" ? "界面强调色" : "UI ACCENT", { fill: palette.muted, size: 11, weight: 720, family: FONT_MONO, spacing: 1.1 })}
    ${uiSwatches}
    ${text(578, 607, lang === "zh" ? "分析图表主色" : "CHART ACCENT", { fill: palette.muted, size: 11, weight: 720, family: FONT_MONO, spacing: 1.1 })}
    ${chartSwatches}
    ${themeScreenshotCard({ id: clips[1].id, x: 1074, y: 398, width: 430, height: 282, title: c.dark, image: dark, darkLabel: true })}
    <line x1="96" y1="718" x2="1504" y2="718" stroke="${palette.line}"/>
    ${facts}
    ${text(1532, 36, c.index, { fill: palette.muted, size: 12.5, weight: 500, family: FONT_MONO, anchor: "end", spacing: 1 })}
  `;
  const extraDefs = `<defs><filter id="shadow-theme-panel-${lang}" x="-10%" y="-15%" width="120%" height="140%"><feDropShadow dx="0" dy="16" stdDeviation="22" flood-color="#372b22" flood-opacity=".09"/></filter><filter id="shadow-theme-light-${lang}" x="-25%" y="-25%" width="150%" height="170%"><feDropShadow dx="0" dy="18" stdDeviation="18" flood-color="#241b15" flood-opacity=".18"/></filter><filter id="shadow-theme-dark-${lang}" x="-25%" y="-25%" width="150%" height="170%"><feDropShadow dx="0" dy="18" stdDeviation="18" flood-color="#241b15" flood-opacity=".18"/></filter>${clips.map((r) => `<clipPath id="clip-${r.id}"><rect x="${r.x}" y="${r.y}" width="${r.width}" height="${r.height}" rx="${r.radius}"/></clipPath>`).join("")}</defs>`;
  return svgFrame({ title: c.title.join(" "), desc: c.lede.join(" "), body, defsContent: extraDefs });
}

function renderVisual(lang, assets) {
  const c = copy[lang].visual;
  const screenshot = assets[`${lang}-visual-light`];
  const clip = { id: `visual-${lang}`, x: 872, y: 376, width: 660, height: 464, radius: 22 };
  const steps = c.steps.map(([n, title, detail], index) => {
    const y = 442 + index * 83;
    const active = index === 1;
    return `<g>
      <rect x="97" y="${y}" width="684" height="68" rx="13" fill="${active ? "#382d28" : palette.dark3}" stroke="${active ? "#8a5744" : "#534b44"}"/>
      <rect x="113" y="${y + 15}" width="34" height="34" rx="9" fill="${active ? palette.accent : "#413a35"}"/>
      ${text(130, y + 37, n, { fill: "#f5e9df", size: 11, weight: 720, family: FONT_MONO, anchor: "middle" })}
      ${text(163, y + 27, title, { fill: palette.lightOnDark, size: 14, weight: 720 })}
      ${text(163, y + 50, detail, { fill: "#bfb3a9", size: lang === "zh" ? 11.5 : 10.4, weight: 470 })}
    </g>`;
  }).join("\n");
  const body = `
    <rect width="1600" height="900" fill="#25211e"/>
    ${eyebrow(c.eyebrow, 68, 72, true)}
    ${textLines(c.title, { x: 68, y: 150, size: lang === "zh" ? 59 : 57, lineHeight: 60, fill: palette.lightOnDark, weight: 560, family: FONT_SERIF, spacing: -1.4 })}
    ${textLines(c.lede, { x: 68, y: 278, size: lang === "zh" ? 19.5 : 18.3, lineHeight: 31, fill: palette.mutedOnDark, weight: 440 })}
    <g transform="translate(1174 58)"><rect width="358" height="40" rx="12" fill="#2d2925" stroke="#5a514a"/>${text(179, 25, c.badge, { fill: "#dcd1c7", size: 11, weight: 720, family: FONT_MONO, anchor: "middle", spacing: 0.9 })}</g>
    <rect x="68" y="376" width="780" height="464" rx="22" fill="#2d2925" stroke="#504841"/>
    ${text(97, 418, c.pipelineTitle, { fill: "#e8ddd3", size: 14, weight: 720 })}
    ${steps}
    <rect x="97" y="786" width="684" height="42" rx="12" fill="#211e1b" stroke="#4b443e"/>
    ${text(116, 804, c.resultTitle, { fill: palette.lightOnDark, size: 12.5, weight: 720 })}
    ${text(762, 804, c.resultBody, { fill: "#94c4a6", size: lang === "zh" ? 9.4 : 8.8, weight: 720, family: FONT_MONO, anchor: "end" })}
    ${roundedImage({ id: clip.id, x: clip.x, y: clip.y, width: clip.width, height: clip.height, dataUri: screenshot, radius: 22, fit: "meet" })}
    <g transform="translate(1337 394)"><rect width="174" height="30" rx="8" fill="rgba(31,28,25,.88)" stroke="rgba(255,255,255,.14)"/>${text(87, 19, c.screenshotLabel, { fill: "#eee2d8", size: lang === "zh" ? 9.5 : 8.8, weight: 720, family: FONT_MONO, anchor: "middle", spacing: 0.5 })}</g>
    ${c.pills.map((item, index) => `<g transform="translate(${918 + index * 142} 792)"><rect width="132" height="30" rx="15" fill="rgba(33,29,26,.9)" stroke="rgba(255,255,255,.14)"/>${text(66, 19, item, { fill: "#e8ddd3", size: 9.2, weight: 720, family: FONT_MONO, anchor: "middle" })}</g>`).join("\n")}
    ${text(1532, 38, c.index, { fill: "#a99d93", size: 12.5, weight: 500, family: FONT_MONO, anchor: "end", spacing: 1 })}
  `;
  return svgFrame({ title: c.title.join(" "), desc: c.lede.join(" "), body, defsContent: defs(`visual-${lang}`, [clip]) });
}

function renderGraph(lang, assets) {
  const c = copy[lang].graph;
  const screenshot = assets[`${lang}-codebase-memory-light`];
  const clip = { id: `graph-${lang}`, x: 68, y: 376, width: 900, height: 410, radius: 18 };
  const flowCards = c.steps.map(([n, title, detail], index) => {
    const y = 400 + index * 76;
    return `<g>
      <rect x="1000" y="${y}" width="532" height="62" rx="14" fill="#fbf8f3" stroke="${palette.line}"/>
      <rect x="1016" y="${y + 14}" width="34" height="34" rx="10" fill="${index === 0 ? palette.accentSoft : palette.paper2}"/>
      ${text(1033, y + 36, n, { fill: index === 0 ? "#8b432d" : palette.ink, size: 10.5, weight: 720, family: FONT_MONO, anchor: "middle" })}
      ${text(1064, y + 25, title, { fill: palette.ink, size: 13.2, weight: 730 })}
      ${text(1064, y + 45, detail, { fill: palette.muted, size: lang === "zh" ? 10.7 : 9.5, weight: 480 })}
    </g>`;
  }).join("\n");
  const footer = c.footerSteps.map((step, index) => {
    const x = 472 + index * 147;
    return `${index > 0 ? text(x - 24, 858, "→", { fill: palette.accent, size: 18, weight: 700, anchor: "middle" }) : ""}<g transform="translate(${x} 836)"><rect width="118" height="34" rx="17" fill="rgba(255,255,255,.67)" stroke="${palette.line}"/>${text(59, 22, step, { fill: "#655b53", size: lang === "zh" ? 11.5 : 10.6, weight: 700, anchor: "middle" })}</g>`;
  }).join("\n");
  const body = `
    <rect width="1600" height="900" fill="#f6f1e9"/>
    ${eyebrow(c.eyebrow, 68, 72)}
    ${textLines(c.title, { x: 68, y: 149, size: lang === "zh" ? 58 : 56, lineHeight: 58, fill: palette.ink, weight: 560, family: FONT_SERIF, spacing: -1.4 })}
    ${textLines(c.lede, { x: 68, y: 273, size: lang === "zh" ? 19 : 17.6, lineHeight: 30, fill: palette.muted, weight: 450 })}
    <g transform="translate(1406 56)"><rect width="126" height="126" rx="28" fill="${palette.panel}" stroke="${palette.line}" filter="url(#graph-mark-${lang})"/><image x="25" y="25" width="76" height="76" href="${assets.icon}"/></g>
    ${text(68, 356, c.screenTitle, { fill: palette.ink, size: 15, weight: 730 })}
    ${text(960, 356, c.screenMeta, { fill: palette.muted, size: 10.5, weight: 700, family: FONT_MONO, anchor: "end", spacing: 0.9 })}
    ${text(1000, 356, c.flowTitle, { fill: palette.ink, size: 15, weight: 730 })}
    ${text(1532, 356, c.flowMeta, { fill: palette.muted, size: 10.5, weight: 700, family: FONT_MONO, anchor: "end", spacing: 0.9 })}
    ${roundedImage({ id: clip.id, x: clip.x, y: clip.y, width: clip.width, height: clip.height, dataUri: screenshot, radius: 18, fit: "meet" })}
    ${flowCards}
    ${footer}
    ${text(1532, 38, c.index, { fill: palette.muted, size: 12.5, weight: 500, family: FONT_MONO, anchor: "end", spacing: 1 })}
  `;
  const graphDefs = `${defs(`graph-${lang}`, [clip]).replace("</defs>", "")}<filter id="graph-mark-${lang}" x="-30%" y="-30%" width="160%" height="160%"><feDropShadow dx="0" dy="12" stdDeviation="18" flood-color="#372b22" flood-opacity=".1"/></filter></defs>`;
  return svgFrame({ title: c.title.join(" "), desc: c.lede.join(" "), body, defsContent: graphDefs });
}

async function rasterizeSvg(svgPath, pngPath, browser) {
  const page = await browser.newPage({ viewport: { width: WIDTH, height: HEIGHT }, deviceScaleFactor: 1 });
  await page.goto(pathToFileURL(svgPath).href, { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: pngPath, fullPage: false, animations: "disabled" });
  await page.close();
}

async function generateArtwork() {
  const icon = await dataUri(iconPath);
  const assets = { icon };
  for (const [lang, pageId, theme] of captureSpecs) {
    assets[`${lang}-${pageId}-${theme}`] = await dataUri(path.join(screenshotsRoot, `${lang}-${pageId}-${theme}.png`));
  }

  const browser = await chromium.launch({ headless: true });
  try {
    for (const lang of ["zh", "en"]) {
      const locale = copy[lang];
      const localeRoot = path.join(outputRoot, locale.localeDir);
      await mkdir(localeRoot, { recursive: true });
      const outputs = [
        ["01-home-overview", renderHero(lang, assets)],
        ["02-theme-system", renderTheme(lang, assets)],
        ["03-visual-compression", renderVisual(lang, assets)],
        ["04-code-graph", renderGraph(lang, assets)],
      ];
      for (const [name, svg] of outputs) {
        const baseName = `d-token-promo-${name}.${locale.localeSuffix}`;
        const svgPath = path.join(localeRoot, `${baseName}.svg`);
        const pngPath = path.join(localeRoot, `${baseName}.png`);
        await writeFile(svgPath, svg, "utf8");
        await rasterizeSvg(svgPath, pngPath, browser);
        console.log(`generated ${path.relative(repoRoot, svgPath)}`);
        console.log(`generated ${path.relative(repoRoot, pngPath)}`);
      }
    }
  } finally {
    await browser.close();
  }
}

await mkdir(outputRoot, { recursive: true });
await captureLatestUi();
await generateArtwork();
