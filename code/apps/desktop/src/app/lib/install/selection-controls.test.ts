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
 * Product boundary: users may choose which Agents join the batch, but d-token
 * still never asks them to choose provider/model/protocol/endpoint/source.
 * Ambiguity fails closed with a rescan/fix path instead of a chooser.
 */
const PRODUCTION_FILES: Record<string, string> = {
  "lib/install/presentation.tsx": resolve(here, "presentation.tsx"),
  "lib/install/InstallView.tsx": resolve(here, "InstallView.tsx"),
  "lib/installMachine.ts": resolve(here, "../installMachine.ts"),
  "components/pages/Install.tsx": resolve(here, "../../components/pages/Install.tsx"),
};

const FORBIDDEN_PATTERNS: Array<{ name: string; pattern: RegExp }> = [
  { name: "Checkbox component", pattern: /Checkbox/ },
  { name: "onSelect handler", pattern: /onSelect/ },
  { name: "SELECT_UPSTREAM event", pattern: /SELECT_UPSTREAM/ },
  { name: "discovered-upstream chooser copy", pattern: /选择已发现/ },
  { name: "needs-choice <select> chooser", pattern: /<select/ },
];

describe("install Agent selection keeps provider/model choosers out of production code", () => {
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

  it("keeps Agent batch selection enabled in the install machine", () => {
    const machineSource = readFileSync(PRODUCTION_FILES["lib/installMachine.ts"], "utf8");
    expect(machineSource).toMatch(/TOGGLE_AGENT/);
    expect(machineSource).toMatch(/SET_AGENT_SELECTION/);
  });

  it("renders Agent checkboxes without rendering an upstream select", () => {
    const viewSource = readFileSync(PRODUCTION_FILES["lib/install/InstallView.tsx"], "utf8");
    const presentationSource = readFileSync(PRODUCTION_FILES["lib/install/presentation.tsx"], "utf8");
    expect(`${viewSource}\n${presentationSource}`).toMatch(/type="checkbox"/);
    expect(`${viewSource}\n${presentationSource}`).not.toMatch(/<select/);
  });
});
