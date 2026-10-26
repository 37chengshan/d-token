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
const FORBIDDEN_PRODUCT_COPY = /审查|审阅|审核|测试|基准|冒烟|测试夹具|模拟(?:数据|服务)|示例|演示|\b(?:review(?:ed|ing)?|audit|test(?:ing)?|fixture|mock|benchmark|smoke|sample|demo)\b/i;
const AI_STYLE_CLICHES = /让我们|我来|为你(?:准备|处理|生成)|无需担心|轻松(?:完成|管理)|\b(?:let'?s|no worries|effortlessly|excited to)\b/i;
const INTERNAL_PRODUCT_RATIONALES = /为减小|不内置|不会代为|代为下载|\b(?:to keep (?:the )?app|does not bundle|download it for you)\b/i;
const UI_SELF_NARRATION = /本页|此页|页面只(?:显示|提供)|应用会|App会|\b(?:this page|the app will|the page will)\b/i;

describe("release UI copy policy", () => {
  it("distinguishes native error matching from returned user copy", () => {
    const source = ts.createSourceFile("userCopy.ts", 'if (normalized.includes("changed after review")) return { message: "Review the changes" };', ts.ScriptTarget.Latest, true);
    const rendered: string[] = [];
    const visit = (node: ts.Node) => {
      if (ts.isStringLiteral(node) && isRenderedCopy(node, path.join(SOURCE_ROOT, "lib/userCopy.ts"))) rendered.push(node.text);
      ts.forEachChild(node, visit);
    };
    visit(source);
    expect(rendered).toEqual(["Review the changes"]);
  });

  it("keeps rendered bilingual text free of development, approval, and AI-style phrasing", () => {
    const violations = sourceFiles(SOURCE_ROOT).flatMap(collectViolations);
    expect(violations).toEqual([]);
  });

  it("does not pass raw native errors or summaries into visible notices", () => {
    const violations = sourceFiles(SOURCE_ROOT).flatMap(collectRawErrorLeaks);
    expect(violations).toEqual([]);
  });
});

function sourceFiles(directory: string): string[] {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry: { name: string; isDirectory(): boolean }) => {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "ui") return [];
      return sourceFiles(file);
    }
    if (!/\.(?:ts|tsx)$/.test(entry.name) || /\.test\.(?:ts|tsx)$/.test(entry.name)) return [];
    return [file];
  });
}

function collectViolations(file: string): string[] {
  const source = ts.createSourceFile(file, fs.readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true, file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const violations: string[] = [];
  const visit = (node: ts.Node) => {
    if (ts.isStringLiteral(node) && isRenderedCopy(node, file)) {
      const text = node.text;
      if (FORBIDDEN_PRODUCT_COPY.test(text) || AI_STYLE_CLICHES.test(text) || INTERNAL_PRODUCT_RATIONALES.test(text) || UI_SELF_NARRATION.test(text)) {
        const position = source.getLineAndCharacterOfPosition(node.getStart(source));
        violations.push(`${path.relative(SOURCE_ROOT, file)}:${position.line + 1}: ${text}`);
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return violations;
}

function isRenderedCopy(node: ts.StringLiteral, file: string) {
  const parent = node.parent;
  if (ts.isCallExpression(parent) && parent.arguments.includes(node)
    && ts.isPropertyAccessExpression(parent.expression)
    && ["includes", "startsWith", "endsWith"].includes(parent.expression.name.text)) return false;
  if (file.endsWith(`${path.sep}i18n.ts`) && ts.isPropertyAssignment(node.parent) && node.parent.name === node) return false;
  if (file.endsWith(`${path.sep}i18n.ts`) || file.endsWith(`${path.sep}userCopy.ts`)) return true;
  let current: ts.Node | undefined = node;
  while (current?.parent) {
    const parent: ts.Node = current.parent;
    if (ts.isPropertyAssignment(parent) && isLocaleKey(parent.name)) return true;
    if (ts.isJsxAttribute(parent) && ts.isIdentifier(parent.name) && ["aria-label", "title", "placeholder"].includes(parent.name.text)) return true;
    if (ts.isConditionalExpression(parent) && /\b(lang|zh)\b/.test(parent.condition.getText())) return true;
    if (ts.isCallExpression(parent) && isVisibleMessageCall(parent)) return true;
    current = parent;
  }
  return false;
}

function isLocaleKey(name: ts.PropertyName) {
  return ts.isIdentifier(name) && (name.text === "zh" || name.text === "en");
}

function isVisibleMessageCall(call: ts.CallExpression) {
  const name = call.expression.getText();
  return ["addNotification", "setNotice", "setError", "setStopError", "setActionFeedback"].includes(name);
}

function collectRawErrorLeaks(file: string): string[] {
  const source = ts.createSourceFile(file, fs.readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true, file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const violations: string[] = [];
  const visit = (node: ts.Node) => {
    if (ts.isCallExpression(node) && isVisibleMessageCall(node)) {
      const candidate = node.arguments[0];
      if (candidate && hasUnsafeNativeText(candidate)) {
        const position = source.getLineAndCharacterOfPosition(candidate.getStart(source));
        violations.push(`${path.relative(SOURCE_ROOT, file)}:${position.line + 1}: raw native error in visible message`);
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return violations;
}

function hasUnsafeNativeText(node: ts.Node): boolean {
  let unsafe = false;
  const visit = (current: ts.Node) => {
    if (unsafe) return;
    if (ts.isCallExpression(current) && current.expression.getText() === "presentOperationFailure") return;
    if (ts.isPropertyAccessExpression(current)
      && ts.isConditionalExpression(current.parent)
      && current.parent.condition === current) return;
    if (ts.isPropertyAccessExpression(current) && current.expression.getText() === "classification") return;
    if (ts.isCallExpression(current) && current.expression.getText() === "String") {
      unsafe = true;
      return;
    }
    if (ts.isPropertyAccessExpression(current) && ["summary", "error"].includes(current.name.text)) {
      unsafe = true;
      return;
    }
    ts.forEachChild(current, visit);
  };
  visit(node);
  return unsafe;
}
