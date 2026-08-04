import { afterEach, describe, expect, it } from "vitest";
import {
  getPerformanceSnapshot,
  initializePerformanceCapture,
  recordPerformanceObservation,
  resetPerformanceCaptureForTests,
} from "./performance";

afterEach(() => resetPerformanceCaptureForTests());

describe("performance capture", () => {
  it("exposes a read-only audit snapshot without creating React subscriptions", () => {
    initializePerformanceCapture();

    expect(window.__DTOKEN_PERFORMANCE__).toBeDefined();
    expect(window.__DTOKEN_PERFORMANCE__?.snapshot()).toEqual(getPerformanceSnapshot());
    expect(Object.keys(window)).not.toContain("__DTOKEN_PERFORMANCE__");
  });

  it("keeps a bounded metadata-only observation window", () => {
    for (let index = 0; index < 300; index += 1) {
      recordPerformanceObservation({
        operation: "native_command",
        phase: "complete",
        duration_us: index,
        result_state: "ok",
      });
    }

    const snapshot = getPerformanceSnapshot();
    expect(snapshot).toHaveLength(256);
    expect(snapshot[0].duration_us).toBe(44);
    expect(snapshot[snapshot.length - 1]?.duration_us).toBe(299);
    expect(JSON.stringify(snapshot)).not.toContain("prompt");
    expect(JSON.stringify(snapshot)).not.toContain("credential");
  });

  it("normalizes negative durations without accepting free-text errors", () => {
    recordPerformanceObservation({
      operation: "native_command",
      phase: "complete",
      duration_us: -5,
      result_state: "error",
      error_code: "native_command_failed",
    });

    expect(getPerformanceSnapshot()[0]).toMatchObject({
      duration_us: 0,
      error_code: "native_command_failed",
    });
  });
});
