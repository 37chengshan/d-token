import { mkdir, readFile } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";
import { chromium } from "@playwright/test";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../../..");
const sourcePath = path.join(repoRoot, "docs/media/promo/source.html");
const outputDir = path.join(repoRoot, "docs/media/promo");

const outputs = [
  ["#promo-01", "d-token-promo-01-product-hero.png"],
  ["#promo-02", "d-token-promo-02-local-architecture.png"],
  ["#promo-03", "d-token-promo-03-context-optimization.png"],
  ["#promo-04", "d-token-promo-04-doctor-rollback.png"],
];

function pngDimensions(buffer) {
  const signature = buffer.subarray(0, 8).toString("hex");
  if (signature !== "89504e470d0a1a0a") {
    throw new Error("Generated artifact is not a PNG file.");
  }
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
  };
}

await mkdir(outputDir, { recursive: true });

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
    colorScheme: "light",
    reducedMotion: "reduce",
  });

  await page.goto(pathToFileURL(sourcePath).href, { waitUntil: "load" });
  await page.waitForFunction(() =>
    Array.from(document.images).every((image) => image.complete && image.naturalWidth > 0),
  );
  await page.evaluate(() => document.fonts.ready);

  for (const [selector, filename] of outputs) {
    const locator = page.locator(selector);
    await locator.waitFor({ state: "visible" });
    const outputPath = path.join(outputDir, filename);
    await locator.screenshot({ path: outputPath, animations: "disabled" });

    const dimensions = pngDimensions(await readFile(outputPath));
    if (dimensions.width !== 1600 || dimensions.height !== 900) {
      throw new Error(
        `${filename} has unexpected dimensions ${dimensions.width}x${dimensions.height}.`,
      );
    }
    console.log(`generated ${path.relative(repoRoot, outputPath)} (${dimensions.width}x${dimensions.height})`);
  }
} finally {
  await browser.close();
}
