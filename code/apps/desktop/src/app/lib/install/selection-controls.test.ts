// The package intentionally omits @types/node; Vitest supplies these modules at runtime.
import { describe, expect, it } from "vitest";
// @ts-ignore Node runtime module
import { readFileSync } from "node:fs";
// @ts-ignore Node runtime module
import { fileURLToPath } from "node:url";
// @ts-ignore Node runtime module
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

/**
 * Product boundary: Agents are user-selectable (PRD P0.0.10.1: installed
 * Agents default to checked, the user may cancel any of them, uninstalled
 * Agents are not checkable), but Providers/models/upstreams are never chosen
 * inside d-token. The install flow auto-resolves provider/model/protocol/
 * endpoint/source; ambiguity fails closed (blocked/partial + rescan), never
 * with a chooser. These file-level guards keep the Agent-level selection
 * controls (Checkbox/onToggle/TOGGLE_AGENT) while forbidding any provider-
 * level chooser from entering the production install surface.
 */
const PRODUCTION_FILES: Record<string, string> = {
  "lib/install/presentation.tsx": resolve(here, "presentation.tsx"),
  "lib/install/InstallView.tsx": resolve(here, "InstallView.tsx"),
  "lib/installMachine.ts": resolve(here, "../installMachine.ts"),
  "components/pages/Install.tsx": resolve(here, "../../components/pages/Install.tsx"),
};

const FORBIDDEN_PATTERNS: Array<{ name: string; pattern: RegExp }> = [
  { name: "SELECT_UPSTREAM event", pattern: /SELECT_UPSTREAM/ },
  { name: "discovered-upstream chooser copy", pattern: /选择已发现/ },
  { name: "choose-model copy", pattern: /Choose[^.]*model/i },
  { name: "needs-choice <select> chooser", pattern: /<select/ },
  { name: "provider/model chooser combobox", pattern: /role="combobox"/ },
  { name: "upstream select element", pattern: /<select\b/i },
];

describe("install selection controls are removed from production code", () => {
  for (const [label, file] of Object.entries(PRODUCTION_FILES)) {
    const source = readFileSync(file, "utf8");
    describe(label, () => {
      for (const { name, pattern } of FORBIDDEN_PATTERNS) {
        it(`does not contain ${name}`, () => {
          expect(source).not.toMatch(pattern);
        });
      }
    });
  }
});
