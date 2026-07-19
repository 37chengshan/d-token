import { readFileSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";

// Eight top-level desktop destinations now share one bilingual shell and
// verified native-control providers. Keep this gate tight enough to catch
// eager regressions, but align it to the current split boundary instead of
// failing every build on the steady-state shell footprint.
const INITIAL_JS_BUDGET_BYTES = 176 * 1024;
const distRoot = new URL("../dist/", import.meta.url);
const manifestPath = fileURLToPath(new URL("../dist/.vite/manifest.json", import.meta.url));

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
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
