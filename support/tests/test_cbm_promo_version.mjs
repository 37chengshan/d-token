/**
 * Node contract test: promo-v2 copy is driven by the v2 Codebase Memory
 * artifact manifest, never by hardcoded version literals.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CBM_MANIFEST_PATH, cbmVersionLabel, loadCbmManifest } from "../../code/apps/desktop/scripts/cbm-promo-version.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../..");
const promoScript = path.join(repoRoot, "code/apps/desktop/scripts/generate-promo-v2.mjs");

test("promo manifest loader reads the pinned v2 manifest", async () => {
  const manifest = await loadCbmManifest();
  assert.equal(manifest.schema_version, 2);
  assert.ok(manifest.upstream.tag, "pinned upstream tag");
  assert.match(manifest.upstream.commit, /^[0-9a-f]{40}$/);
});

test("version label is the manifest upstream tag", async () => {
  const manifest = await loadCbmManifest();
  assert.equal(cbmVersionLabel(manifest), manifest.upstream.tag);
});

test("promo script interpolates the manifest label and has no hardcoded version", async () => {
  const source = await readFile(promoScript, "utf8");
  const manifest = await loadCbmManifest();
  assert.ok(
    source.includes(`Codebase Memory MCP \${cbmVersionLabel(promoManifest)}`),
    "promo copy must interpolate the manifest-driven label",
  );
  for (const literal of ["v0.9.1-rc.1", "v0.9.0", "0.8.1"]) {
    assert.ok(
      !source.includes(`Codebase Memory MCP ${literal}`),
      `hardcoded version literal must not appear in promo copy: ${literal}`,
    );
  }
  assert.equal(cbmVersionLabel(manifest), manifest.upstream.tag);
});

test("promo loader rejects non-v2 manifests fail-closed", async () => {
  const { mkdtemp, writeFile, rm } = await import("node:fs/promises");
  const { tmpdir } = await import("node:os");
  const tmp = await mkdtemp(path.join(tmpdir(), "cbm-promo-"));
  try {
    const v1 = path.join(tmp, "manifest-v1.json");
    await writeFile(v1, JSON.stringify({ schema_version: 1, upstream: { tag: "v0.9.0" } }));
    await assert.rejects(() => loadCbmManifest(v1), /schema_version=2/);
  } finally {
    await rm(tmp, { recursive: true, force: true });
  }
});

test("manifest file tracked in repo is the one the promo reads", () => {
  assert.equal(CBM_MANIFEST_PATH, path.join(repoRoot, "code/runtimes/codebase-memory/artifact-manifest.json"));
});
