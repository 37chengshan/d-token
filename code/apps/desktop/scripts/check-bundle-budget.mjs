import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";

// Eight top-level desktop destinations now share one bilingual shell and
// verified native-control providers. Keep this gate tight enough to catch
// eager regressions, but align it to the current split boundary instead of
// failing every build on the steady-state shell footprint.
const INITIAL_JS_BUDGET_BYTES = 176 * 1024;
const jsonOutputIndex = process.argv.indexOf("--json-output");
const jsonOutputPath = jsonOutputIndex >= 0 ? process.argv[jsonOutputIndex + 1] : undefined;
if (jsonOutputIndex >= 0 && !jsonOutputPath) {
  throw new Error("bundle_gate_json_output_path_missing");
}
const distRoot = new URL("../dist/", import.meta.url);
const manifestPath = fileURLToPath(new URL("../dist/.vite/manifest.json", import.meta.url));

const manifestBytes = readFileSync(manifestPath);
const manifest = JSON.parse(manifestBytes.toString("utf8"));
const entryKey = Object.keys(manifest).find((key) => manifest[key]?.isEntry && manifest[key]?.src === "index.html")
  ?? Object.keys(manifest).find((key) => manifest[key]?.isEntry);

if (!entryKey) {
  throw new Error(`bundle_gate_missing_entry:${manifestPath}`);
}

const staticEntries = new Set();
const visit = (key) => {
  if (staticEntries.has(key)) return;
  const entry = manifest[key];
  if (!entry) throw new Error(`bundle_gate_missing_manifest_import:${key}`);
  staticEntries.add(key);
  for (const importedKey of entry.imports ?? []) visit(importedKey);
};
visit(entryKey);

const initialJavaScript = [...staticEntries]
  .map((key) => ({ key, entry: manifest[key] }))
  .filter(({ entry }) => entry.file?.endsWith(".js"))
  .map(({ key, entry }) => {
    const bytes = readFileSync(new URL(entry.file, distRoot));
    return {
      key,
      file: entry.file,
      gzipBytes: gzipSync(bytes, { level: 9 }).byteLength,
    };
  })
  .sort((left, right) => right.gzipBytes - left.gzipBytes);

const initialGzipBytes = initialJavaScript.reduce((total, asset) => total + asset.gzipBytes, 0);
const eagerCharts = initialJavaScript.filter(({ key, file }) => `${key}:${file}`.includes("charts-vendor"));

for (const asset of initialJavaScript) {
  console.log(`${String(asset.gzipBytes).padStart(7)} B  ${asset.file}`);
}
console.log(`Initial recursive JS: ${initialGzipBytes} B / ${INITIAL_JS_BUDGET_BYTES} B (${(initialGzipBytes / 1024).toFixed(2)} KiB / ${(INITIAL_JS_BUDGET_BYTES / 1024).toFixed(2)} KiB)`);

if (eagerCharts.length > 0) {
  console.error(`Bundle gate failed: charts must remain lazy (${eagerCharts.map(({ file }) => file).join(", ")}).`);
  process.exitCode = 1;
}
if (initialGzipBytes > INITIAL_JS_BUDGET_BYTES) {
  console.error(`Bundle gate failed: initial recursive JS exceeds the budget by ${initialGzipBytes - INITIAL_JS_BUDGET_BYTES} B.`);
  process.exitCode = 1;
}

const failedChecks = [
  ...(eagerCharts.length > 0 ? ["charts_remain_lazy"] : []),
  ...(initialGzipBytes > INITIAL_JS_BUDGET_BYTES ? ["initial_recursive_js_gzip_bytes"] : []),
];
const report = {
  schema_version: 1,
  scope: "desktop-bundle-budget",
  manifest: "dist/.vite/manifest.json",
  manifest_sha256: createHash("sha256").update(manifestBytes).digest("hex"),
  initial_recursive_js_gzip_bytes: initialGzipBytes,
  initial_recursive_js_budget_bytes: INITIAL_JS_BUDGET_BYTES,
  initial_asset_count: initialJavaScript.length,
  eager_chart_asset_count: eagerCharts.length,
  assets: initialJavaScript.map(({ file, gzipBytes }) => ({ file, gzip_bytes: gzipBytes })),
  gate: { ok: failedChecks.length === 0, failed_checks: failedChecks },
};
if (jsonOutputPath) {
  writeFileSync(jsonOutputPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
}
