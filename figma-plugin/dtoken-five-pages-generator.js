// d-token five-page review canvas generator
// Import with Figma desktop: Plugins > Development > New plugin from manifest,
// point at figma-plugin/manifest.generator.json, then run this file from the
// plugin's code entry when prompted. No network or credentials are required.
//
// The script is intentionally self-contained so it can be pasted into the
// Figma console/bridge EXECUTE_CODE action when the bridge is connected.

const DT = {
  paper: { r: 0.965, g: 0.949, b: 0.918 },
  paper2: { r: 0.988, g: 0.978, b: 0.949 },
  ink: { r: 0.161, g: 0.145, b: 0.122 },
  muted: { r: 0.443, g: 0.416, b: 0.380 },
  line: { r: 0.286, g: 0.239, b: 0.184 },
  orange: { r: 0.78, g: 0.29, b: 0.10 },
  orangeSoft: { r: 0.96, g: 0.86, b: 0.76 },
  green: { r: 0.12, g: 0.48, b: 0.28 },
  greenSoft: { r: 0.84, g: 0.94, b: 0.86 },
  amber: { r: 0.62, g: 0.38, b: 0.05 },
  amberSoft: { r: 0.98, g: 0.91, b: 0.70 },
  red: { r: 0.68, g: 0.16, b: 0.13 },
  redSoft: { r: 0.98, g: 0.85, b: 0.83 },
  blue: { r: 0.18, g: 0.36, b: 0.63 },
  blueSoft: { r: 0.84, g: 0.90, b: 0.98 },
};
const solid = (c, opacity = 1) => ({ type: 'SOLID', color: c, opacity });
const paint = (c, opacity = 1) => [solid(c, opacity)];
const font = { family: 'Inter', style: 'Regular' };
const semibold = { family: 'Inter', style: 'Semi Bold' };
const mono = { family: 'Roboto Mono', style: 'Regular' };

async function dtLoadFonts() {
  for (const f of [font, semibold, mono]) {
    try { await figma.loadFontAsync(f); } catch (_) { /* fallback is handled by Figma */ }
  }
}
function text(parent, value, x, y, size = 14, color = DT.ink, opts = {}) {
  const n = figma.createText(); n.fontName = opts.mono ? mono : (opts.bold ? semibold : font);
  n.fontSize = size; n.characters = value; n.fills = paint(color, opts.opacity ?? 1);
  n.x = x; n.y = y; n.name = opts.name || value.slice(0, 32);
  if (opts.width) { n.textAutoResize = 'HEIGHT'; n.resize(opts.width, n.height); }
  parent.appendChild(n); return n;
}
function rect(parent, x, y, w, h, fill, radius = 12, stroke = null, opacity = 1) {
  const n = figma.createRectangle(); n.x = x; n.y = y; n.resize(w, h); n.cornerRadius = radius;
  n.fills = paint(fill, opacity); if (stroke) { n.strokes = paint(stroke); n.strokeWeight = 1; }
  parent.appendChild(n); return n;
}
function line(parent, x, y, w, color = DT.line) { return rect(parent, x, y, w, 1, color, 0, null, 0.14); }
function pill(parent, label, x, y, bg, fg = DT.ink, w = null) {
  const width = w || Math.max(52, label.length * 7 + 20); rect(parent, x, y, width, 24, bg, 12);
  text(parent, label, x + 10, y + 6, 11, fg, { bold: true }); return width;
}
function card(parent, x, y, w, h, title, subtitle = '') {
  rect(parent, x, y, w, h, DT.paper2, 12, DT.line, 1);
  text(parent, title, x + 18, y + 16, 14, DT.ink, { bold: true });
  if (subtitle) text(parent, subtitle, x + 18, y + 38, 11, DT.muted, { width: w - 36 });
}
function nav(parent, active) {
  rect(parent, 0, 0, 184, 820, DT.ink, 0);
  text(parent, 'd-token', 22, 25, 20, DT.paper2, { bold: true });
  text(parent, 'LOCAL CONTROL PLANE', 22, 53, 9, DT.orangeSoft, { bold: true });
  const items = ['Configuration', 'Advanced', 'Analysis & Logs', 'Diagnostics', 'Settings'];
  items.forEach((item, i) => {
    const y = 104 + i * 48;
    if (item === active) rect(parent, 12, y - 6, 160, 36, DT.orange, 9);
    text(parent, item, 34, y + 4, 12, item === active ? DT.paper2 : DT.paper, { bold: item === active });
    text(parent, ['⌘', '⌁', '▦', '✚', '⚙'][i], 18, y + 4, 12, item === active ? DT.paper2 : DT.muted);
  });
  line(parent, 22, 686, 140, DT.paper);
  text(parent, 'RUNTIME', 22, 708, 9, DT.muted, { bold: true });
  text(parent, '●  localhost:18001', 22, 730, 11, DT.greenSoft, { mono: true });
  text(parent, 'evidence: partial', 22, 750, 10, DT.amberSoft, { mono: true });
}
function shell(title, eyebrow, active) {
  const f = figma.createFrame(); f.name = title; f.resize(1280, 820); f.fills = paint(DT.paper); f.clipsContent = true;
  nav(f, active); text(f, eyebrow.toUpperCase(), 220, 26, 10, DT.orange, { bold: true });
  text(f, title, 220, 48, 26, DT.ink, { bold: true });
  text(f, 'Evidence first · every state is explicit', 220, 84, 12, DT.muted);
  rect(f, 220, 112, 1032, 1, DT.line, 0, null, 0.12);
  return f;
}
function statusBar(f, label, detail, state = 'partial') {
  const bg = state === 'healthy' ? DT.greenSoft : state === 'error' ? DT.redSoft : state === 'loading' ? DT.blueSoft : DT.amberSoft;
  const fg = state === 'healthy' ? DT.green : state === 'error' ? DT.red : state === 'loading' ? DT.blue : DT.amber;
  rect(f, 220, 128, 1032, 54, bg, 10); pill(f, state.toUpperCase(), 238, 143, fg, DT.paper2, 80);
  text(f, label, 334, 140, 12, DT.ink, { bold: true }); text(f, detail, 334, 159, 10, DT.muted, { mono: true });
}
function button(f, label, x, y, primary = false, w = 118) { rect(f, x, y, w, 32, primary ? DT.orange : DT.paper2, 8, primary ? null : DT.line); text(f, label, x + 14, y + 9, 11, primary ? DT.paper2 : DT.ink, { bold: true }); }
function sectionLabel(f, label, x, y) { text(f, label.toUpperCase(), x, y, 10, DT.muted, { bold: true }); }
function evidence(f, x, y, w, source, freshness, level = 'partial') {
  rect(f, x, y, w, 30, DT.paper, 8, DT.line); text(f, 'Evidence', x + 12, y + 9, 10, DT.muted, { bold: true });
  text(f, source, x + 76, y + 9, 10, DT.ink, { mono: true }); pill(f, `${level} · ${freshness}`, x + w - 130, y + 3, level === 'healthy' ? DT.greenSoft : DT.amberSoft, level === 'healthy' ? DT.green : DT.amber, 118);
}
function row(f, x, y, w, title, detail, status, bg = DT.paper2) {
  rect(f, x, y, w, 54, bg, 8, DT.line); text(f, title, x + 14, y + 11, 12, DT.ink, { bold: true }); text(f, detail, x + 14, y + 31, 10, DT.muted, { mono: true }); pill(f, status, x + w - 116, y + 15, status === 'healthy' ? DT.greenSoft : status === 'error' ? DT.redSoft : DT.amberSoft, status === 'healthy' ? DT.green : status === 'error' ? DT.red : DT.amber, 96);
}
function stateStrip(f, x, y, w) {
  sectionLabel(f, 'Reusable state strip', x, y); ['loading', 'empty', 'healthy', 'partial', 'unavailable', 'error'].forEach((s, i) => {
    const colors = { loading: DT.blueSoft, empty: DT.paper, healthy: DT.greenSoft, partial: DT.amberSoft, unavailable: DT.paper, error: DT.redSoft };
    pill(f, s, x + i * (w / 6), y + 18, colors[s], DT.ink, Math.min(112, w / 6 - 8));
  });
}
function configPage() {
  const f = shell('Configuration', '01 · configure the runtime', 'Configuration'); statusBar(f, 'Local compression engine', 'source: d-token runtime · freshness: 12s', 'partial');
  card(f, 220, 202, 500, 238, 'Compression Engine', 'Choose a mode, then verify the evidence that powers it.');
  pill(f, 'LOCAL', 240, 258, DT.orangeSoft, DT.orange, 76); text(f, 'Deterministic local rules', 334, 261, 14, DT.ink, { bold: true });
  text(f, 'Safe default · no external credential required', 240, 296, 11, DT.muted);
  button(f, 'Selected', 240, 334, true, 104); button(f, 'Test connection', 354, 334, false, 128); evidence(f, 240, 386, 440, 'runtime + ledger', '12s ago', 'partial');
  card(f, 740, 202, 512, 238, 'Code Graph', 'Optional repository map for context ranking and navigation.');
  row(f, 760, 258, 472, 'Repository index', 'workspace: d-token · files: unknown', 'unavailable'); button(f, 'Scan repository', 760, 346, true, 128); button(f, 'View scope', 898, 346, false, 104); evidence(f, 760, 386, 472, 'native command', 'never', 'unavailable');
  sectionLabel(f, 'Agent setup flow', 220, 474); row(f, 220, 492, 1032, 'OpenCode', 'detected · route: d-token local · last request redacted', 'healthy'); row(f, 220, 554, 1032, 'Claude Code / Cursor', 'not configured · one-click setup preserves a recoverable backup', 'partial');
  stateStrip(f, 220, 654, 1032); return f;
}
function advancedPage() {
  const f = shell('Advanced Configuration', '02 · shape context decisions', 'Advanced'); statusBar(f, 'Jev workflow policy', '5 workflows · shadow mode · fallback: local heuristic', 'partial');
  card(f, 220, 202, 660, 352, 'Workflow policy', 'Each workflow shows mode, fallback, and last evidence.');
  const flows = ['Tool output retention', 'Arrival span retention (Sticky)', 'Session compaction', 'Context ranking', 'Rule & skill selection'];
  flows.forEach((name, i) => row(f, 240, 258 + i * 54, 620, name, i < 4 ? 'shadow · fallback local-heuristic' : 'disabled · fallback passthrough', i < 4 ? 'partial' : 'unavailable', DT.paper));
  card(f, 900, 202, 352, 168, 'Smart learning', 'Retain only redacted decisions with a visible retention window.');
  row(f, 920, 260, 312, 'Learning state', 'off · user preference', 'healthy'); button(f, 'Review policy', 920, 314, false, 112);
  card(f, 900, 388, 352, 166, 'Routing & cache', 'No silent fallback; show route provenance before apply.'); row(f, 920, 446, 312, 'Cache', 'local · TTL 30m · 0 hits', 'partial'); button(f, 'Clear cache', 920, 500, false, 104);
  card(f, 220, 582, 1032, 104, 'Recovery & retention', 'Every write includes summary, backup, verification, and rollback evidence.'); text(f, 'Retention 30 days', 240, 636, 12, DT.ink, { bold: true }); pill(f, 'redaction: high', 390, 630, DT.greenSoft, DT.green, 116); button(f, 'Preview policy', 1080, 624, false, 132); stateStrip(f, 220, 716, 1032); return f;
}
function analysisPage() {
  const f = shell('Analysis & Logs', '03 · inspect decisions', 'Analysis & Logs'); statusBar(f, 'Event Ledger', 'redacted request timeline · 24h window · 18 requests', 'healthy');
  ['Overview', 'Requests', 'Changes', 'Logs'].forEach((t, i) => pill(f, t, 220 + i * 108, 202, i === 0 ? DT.orangeSoft : DT.paper2, i === 0 ? DT.orange : DT.ink, 96));
  card(f, 220, 256, 328, 132, 'Requests', 'Decision coverage across routed requests.'); text(f, '18', 240, 300, 34, DT.ink, { bold: true }); text(f, 'last 24 hours', 240, 344, 11, DT.muted);
  card(f, 568, 256, 328, 132, 'Context saved', 'Measured only when runtime evidence is present.'); text(f, '—', 588, 300, 34, DT.muted, { bold: true }); text(f, 'unavailable in preview', 588, 344, 11, DT.amber);
  card(f, 916, 256, 336, 132, 'Changes', 'Applied configuration transactions.'); text(f, '3', 936, 300, 34, DT.ink, { bold: true }); text(f, '2 verified · 1 partial', 936, 344, 11, DT.muted);
  sectionLabel(f, 'Redacted request timeline', 220, 430); row(f, 220, 450, 1032, '15:42:10 · OpenCode → deepseek-coder', 'route local · prompt/response redacted · correlation: req_••••', 'healthy'); row(f, 220, 512, 1032, '15:39:22 · Cursor → provider', 'route unavailable · upstream evidence stale · correlation: req_••••', 'partial'); row(f, 220, 574, 1032, '15:21:08 · OpenCode → provider', 'decision failed · retry not automatic · correlation: req_••••', 'error'); evidence(f, 220, 650, 1032, 'event-ledger', 'freshness 12s', 'healthy'); stateStrip(f, 220, 708, 1032); return f;
}
function diagnosticsPage() {
  const f = shell('Diagnostics', '04 · explain and repair', 'Diagnostics'); statusBar(f, 'Runtime health', '127.0.0.1:18001 · PID identity verified · provider evidence partial', 'partial');
  const cards = [['Runtime', 'loopback listener · port owner verified', 'healthy'], ['Provider', 'upstream reachability · last probe 2m ago', 'partial'], ['Jev', 'credential absent · local mode remains valid', 'unavailable'], ['Agent', 'OpenCode route installed · config backup present', 'healthy'], ['Security', 'redaction high · auth never enters ledger', 'healthy'], ['Recovery', 'last transaction rollback evidence available', 'partial']];
  cards.forEach((c, i) => { const x = 220 + (i % 2) * 520, y = 210 + Math.floor(i / 2) * 112; card(f, x, y, 496, 94, c[0], c[1]); pill(f, c[2], x + 18, y + 58, c[2] === 'healthy' ? DT.greenSoft : c[2] === 'partial' ? DT.amberSoft : DT.paper, c[2] === 'healthy' ? DT.green : c[2] === 'partial' ? DT.amber : DT.muted, 100); });
  card(f, 220, 564, 1032, 106, 'Diagnostic action', 'Actions are scoped, reversible, and report a result with a correlation id.'); text(f, 'Provider probe is stale; refresh before considering a repair.', 240, 612, 12, DT.ink, { bold: true }); button(f, 'Refresh checks', 1040, 610, true, 124); evidence(f, 220, 690, 1032, 'native command + ledger', '2m ago', 'partial'); stateStrip(f, 220, 748, 1032); return f;
}
function settingsPage() {
  const f = shell('Settings', '05 · application preferences', 'Settings'); statusBar(f, 'Application preferences', 'appearance and retention · no provider routing here', 'healthy');
  card(f, 220, 202, 500, 202, 'Appearance', 'Keep product-level visual controls together.'); row(f, 240, 260, 460, 'Theme', 'system · light / dark / system', 'healthy'); row(f, 240, 322, 460, 'Accent', 'orange · chart accent follows', 'healthy');
  card(f, 740, 202, 512, 202, 'Privacy & retention', 'Redaction and retention are application policies.'); row(f, 760, 260, 472, 'Redaction', 'high · credentials excluded', 'healthy'); row(f, 760, 322, 472, 'Event retention', '30 days · local only', 'healthy');
  card(f, 220, 436, 1032, 170, 'Service lifecycle', 'Start, stop, and autostart are explicit system actions.'); row(f, 240, 494, 640, 'd-token service', 'running · managed by d-token · localhost only', 'healthy'); button(f, 'Service settings', 1040, 492, false, 132); evidence(f, 240, 562, 640, 'runtime', '12s ago', 'healthy');
  card(f, 220, 632, 1032, 74, 'About', 'Version and support links belong at the end of the page.'); text(f, 'd-token desktop · build metadata available in runtime', 240, 674, 11, DT.muted, { mono: true }); stateStrip(f, 220, 734, 1032); return f;
}
function componentSheet() {
  const f = figma.createFrame(); f.name = '00 · Components & interaction states'; f.resize(1280, 600); f.fills = paint(DT.paper); text(f, 'Reusable components & states', 32, 28, 24, DT.ink, { bold: true });
  const items = [['Status bar', 'announce state + source + freshness'], ['Evidence bar', 'never imply healthy without source'], ['Workflow row', 'title → policy → fallback → state'], ['Change preview dialog', 'summary → diff → backup → apply'], ['Diagnostic action card', 'scope → reversible action → result'], ['Request timeline', 'redacted ids + route provenance']];
  items.forEach((it, i) => { const x = 32 + (i % 3) * 402, y = 90 + Math.floor(i / 3) * 190; card(f, x, y, 370, 144, it[0], it[1]); pill(f, i % 2 ? 'keyboard reachable' : 'aria-live', x + 18, y + 84, DT.orangeSoft, DT.orange, 126); text(f, 'Focus order: heading → control → result', x + 18, y + 116, 10, DT.muted, { mono: true }); }); return f;
}
async function main() {
  await dtLoadFonts();
  const old = figma.root.children.find(n => n.type === 'PAGE' && n.name === 'd-token · Five-page review');
  if (old) old.remove();
  const page = figma.createPage(); page.name = 'd-token · Five-page review'; figma.currentPage = page;
  const frames = [configPage(), advancedPage(), analysisPage(), diagnosticsPage(), settingsPage(), componentSheet()];
  frames.forEach((f, i) => { f.x = (i % 2) * 1360; f.y = Math.floor(i / 2) * 900; });
  figma.currentPage.selection = frames; figma.viewport.scrollAndZoomIntoView(frames);
  figma.notify('d-token five-page review canvas created');
  return { pageId: page.id, pageName: page.name, frameIds: frames.map(f => ({ id: f.id, name: f.name })) };
}
main();
