/**
 * Manifest-driven Codebase Memory version label for promo artwork.
 *
 * Promo copy must never hardcode an upstream version: the single source of
 * truth is `code/runtimes/codebase-memory/artifact-manifest.json` (schema v2).
 * This module only reads the manifest; it never downloads or executes
 * anything.
 */
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(desktopRoot, "../../..");

export const CBM_MANIFEST_PATH = path.join(
  repoRoot,
  "code",
  "runtimes",
  "codebase-memory",
  "artifact-manifest.json",
);

/** Load the v2 manifest and return the pinned upstream tag/commit. */
export async function loadCbmManifest(manifestPath = CBM_MANIFEST_PATH) {
  const raw = await readFile(manifestPath, "utf8");
  const manifest = JSON.parse(raw);
  if (manifest.schema_version !== 2) {
    throw new Error(
      `codebase-memory promo requires the v2 artifact manifest (schema_version=2), got ${manifest.schema_version}`,
    );
  }
  if (!manifest.upstream?.tag || !manifest.upstream?.commit) {
    throw new Error("codebase-memory v2 manifest has no pinned upstream tag/commit");
  }
  return manifest;
}

/** Human-readable pinned version label, e.g. "v0.9.1-rc.1". */
export function cbmVersionLabel(manifest) {
  return manifest.upstream.tag;
}
