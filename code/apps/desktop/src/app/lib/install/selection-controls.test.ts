import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

/**
 * Product boundary: Agents, upstreams, and models are never chosen by the
 * user inside d-token. The install flow auto-includes every installed Agent
 * with a verified write action and resolves provider/model/protocol/endpoint/
 * source automatically. Ambiguity fails closed (blocked/partial + rescan),
 * never with a chooser. These file-level guards keep the selection controls
 * from regressing into the production install surface.
 */
const PRODUCTION_FILES: Record<string, string> = {
  "lib/install/presentation.tsx": resolve(here, "presentation.tsx"),
  "lib/install/InstallView.tsx": resolve(here, "InstallView.tsx"),
  "lib/installMachine.ts": resolve(here, "../installMachine.ts"),
  "components/pages/Install.tsx": resolve(here, "../../components/pages/Install.tsx"),
};

const FORBIDDEN_PATTERNS: Array<{ name: string; pattern: RegExp }> = [
  { name: "Checkbox component", pattern: /Checkbox/ },
  { name: "onToggle handler", pattern: /onToggle/ },
  { name: "onSelect handler", pattern: /onSelect/ },
  { name: "SELECT_UPSTREAM event", pattern: /SELECT_UPSTREAM/ },
  { name: "TOGGLE_AGENT event", pattern: /TOGGLE_AGENT/ },
  { name: "discovered-upstream chooser copy", pattern: /选择已发现/ },
  { name: "choose-model copy", pattern: /Choose[^.]*model/i },
  { name: "needs-choice <select> chooser", pattern: /<select/ },
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
