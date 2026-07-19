import { afterEach, describe, expect, it } from "vitest";
import { readSessionTab, writeSessionTab } from "./sessionTab";

const KEY = "d-token.test.tab";
const ALLOWED = ["a", "b", "c"] as const;

afterEach(() => {
  try {
    window.sessionStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
});

describe("sessionTab", () => {
  it("returns fallback when nothing is stored", () => {
    expect(readSessionTab(KEY, ALLOWED, "a")).toBe("a");
  });

  it("reads a valid stored tab", () => {
    writeSessionTab(KEY, "b");
    expect(readSessionTab(KEY, ALLOWED, "a")).toBe("b");
  });

  it("ignores values outside the allow-list", () => {
    writeSessionTab(KEY, "evil");
    expect(readSessionTab(KEY, ALLOWED, "a")).toBe("a");
  });
});
