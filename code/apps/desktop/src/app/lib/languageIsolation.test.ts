// Language-isolation contract for active desktop surfaces.
//
// User-visible copy in the eight active pages, the onboarding wizard, and the
// shared common surfaces must live in the central DICT and be rendered through
// translateCopy. This test is static (AST-based) on purpose: it catches a
// future component introducing a bilingual literal escape before any test
// renderer would, while explicitly allowing:
//   - locale selection (e.g. `lang === "zh" ? "zh-CN" : "en"`),
//   - machine identifiers (identical zh/en strings such as "OpenAI"),
//   - data switches over machine fields (e.g. `line.msg : line.msgEn`).
//
// This package intentionally has no @types/node dependency. Vitest executes
// these imports in Node; keep the source guard self-contained.
// @ts-ignore Node runtime module
import fs from "node:fs";
// @ts-ignore Node runtime module
import path from "node:path";
import ts from "typescript";
import { describe, expect, it } from "vitest";

declare const process: { cwd(): string };

const SOURCE_ROOT = path.resolve(process.cwd(), "src/app");

// The migrated active surfaces: eight pages + onboarding + shared common/shell
// surfaces that render their own copy. Files outside this list are out of
// contract scope (inactive pages, ui kit, tests, and the dictionary itself).
const SCOPED_FILES = [
  "TrayPanelApp.tsx",
  "components/common/CapabilitySummary.tsx",
  "components/common/Heatmap.tsx",
  "components/common/NotificationCenter.tsx",
  "components/common/RuntimeNotificationBridge.tsx",
  "components/common/Terminal.tsx",
  "components/shell/AppShell.tsx",
  "components/shell/StatusBar.tsx",
  "components/pages/Onboarding.tsx",
  "components/pages/Overview.tsx",
  "components/pages/Install.tsx",
  "components/pages/Analytics.tsx",
  "components/pages/Changes.tsx",
  "components/pages/Settings.tsx",
  "lib/overview/components.tsx",
  "lib/overview/derivations.ts",
  "lib/overview/savings.ts",
  "lib/install/InstallView.tsx",
  "lib/install/presentation.tsx",
  "lib/visual-compression/VisualCompressionWorkbench.tsx",
  "lib/optical/configDraft.ts",
  "lib/optical/format.ts",
  "lib/doctor/DoctorWorkbench.tsx",
  "lib/protocol.ts",
];

const LOCALE_TAG = /^[a-z]{2}(-[A-Za-z]{2,4})?$/;
const LANG_COND = /\blang\s*===\s*["']zh["']/;
const ZH_COND = /^zh$/;

function stringText(node: ts.Node, source: ts.SourceFile): string | null {
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return node.text;
  if (ts.isTemplateExpression(node)) {
    let text = node.head.text;
    for (const span of node.templateSpans) {
      text += `{expr}` + span.literal.text;
    }
    return text;
  }
  return null;
}

describe("language isolation contract", () => {
  it("keeps active-surface bilingual copy in the dictionary (no literal zh/en escapes)", () => {
    const violations: string[] = [];
    for (const rel of SCOPED_FILES) {
      const file = path.join(SOURCE_ROOT, rel);
      if (!fs.existsSync(file)) continue;
      const sourceText = fs.readFileSync(file, "utf8");
      const source = ts.createSourceFile(rel, sourceText, ts.ScriptTarget.Latest, true, rel.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
      const visit = (node: ts.Node) => {
        if (ts.isConditionalExpression(node)) {
          const condition = node.condition.getText(source);
          const isLang = LANG_COND.test(condition);
          const isZh = ZH_COND.test(condition) && /\bconst\s+zh\s*=\s*lang\s*===\s*["']zh["']/.test(sourceText);
          if (isLang || isZh) {
            const whenTrue = stringText(node.whenTrue, source);
            const whenFalse = stringText(node.whenFalse, source);
            if (whenTrue !== null && whenFalse !== null
              && whenTrue !== whenFalse
              && !(LOCALE_TAG.test(whenTrue) && LOCALE_TAG.test(whenFalse))) {
              const position = source.getLineAndCharacterOfPosition(node.getStart(source));
              violations.push(`${rel}:${position.line + 1}: "${whenTrue}" / "${whenFalse}"`);
            }
          }
        }
        if (ts.isPropertyAssignment(node) && ts.isIdentifier(node.name) && node.name.text === "zh"
          && ts.isPropertyAssignment(node.parent)
          && ts.isIdentifier((node.parent as ts.PropertyAssignment).name)
          && (node.parent as ts.PropertyAssignment).name.text === "en") {
          const object = node.parent.parent;
          if (ts.isObjectLiteralExpression(object)) {
            const zh = propertyString(object, "zh");
            const en = propertyString(object, "en");
            if (zh !== null && en !== null && zh !== en && !(LOCALE_TAG.test(zh) && LOCALE_TAG.test(en))) {
              const position = source.getLineAndCharacterOfPosition(object.getStart(source));
              violations.push(`${rel}:${position.line + 1}: label map "${zh}" / "${en}"`);
            }
          }
        }
        ts.forEachChild(node, visit);
      };
      visit(source);
    }
    expect(violations).toEqual([]);
  });

  it("keeps every translateCopy template placeholder braced and provided", () => {
    const problems: string[] = [];
    const entries = parseDict();
    const callNames = collectCallValueNames();
    for (const [key, entry] of entries) {
      const names = callNames.get(key);
      if (!names || names.size === 0) continue;
      for (const langText of [entry.zh, entry.en]) {
        for (const bare of langText.matchAll(/(?<!\{)v([0-9]+)/g)) {
          if (names.has(`v${bare[1]}`)) {
            problems.push(`${key}: bare v${bare[1]} in "${langText.slice(0, 60)}"`);
          }
        }
        for (const ph of langText.matchAll(/\{v([0-9]+)\}/g)) {
          if (!names.has(`v${ph[1]}`)) {
            problems.push(`${key}: {v${ph[1]}} not provided by call site`);
          }
        }
      }
    }
    expect(problems).toEqual([]);
  });
});

function parseDict(): Map<string, { zh: string; en: string }> {
  const file = path.join(SOURCE_ROOT, "lib", "i18n.ts");
  const source = ts.createSourceFile("i18n.ts", fs.readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const entries = new Map<string, { zh: string; en: string }>();
  const visit = (node: ts.Node) => {
    if (ts.isPropertyAssignment(node) && ts.isObjectLiteralExpression(node.initializer)) {
      const key = node.name.getText(source);
      const zh = propertyString(node.initializer, "zh");
      const en = propertyString(node.initializer, "en");
      if (key.includes(".") && zh !== null && en !== null) entries.set(key, { zh, en });
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return entries;
}

function collectCallValueNames(): Map<string, Set<string>> {
  const names = new Map<string, Set<string>>();
  const scan = (file: string) => {
    const text = fs.readFileSync(file, "utf8");
    const source = ts.createSourceFile(path.basename(file), text, ts.ScriptTarget.Latest, true, file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
    const visit = (node: ts.Node) => {
      if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "translateCopy"
        && node.arguments.length >= 3 && ts.isStringLiteral(node.arguments[0]) && ts.isObjectLiteralExpression(node.arguments[2])) {
        const key = node.arguments[0].text;
        if (!names.has(key)) names.set(key, new Set());
        for (const prop of node.arguments[2].properties) {
          if (ts.isPropertyAssignment(prop) && ts.isIdentifier(prop.name)) names.get(key)!.add(prop.name.text);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  };
  for (const rel of SCOPED_FILES) {
    const file = path.join(SOURCE_ROOT, rel);
    if (fs.existsSync(file)) scan(file);
  }
  return names;
}

function propertyString(object: ts.ObjectLiteralExpression, name: string): string | null {
  for (const prop of object.properties) {
    if (ts.isPropertyAssignment(prop) && ts.isIdentifier(prop.name) && prop.name.text === name
      && (ts.isStringLiteral(prop.initializer) || ts.isNoSubstitutionTemplateLiteral(prop.initializer))) {
      return prop.initializer.text;
    }
  }
  return null;
}
