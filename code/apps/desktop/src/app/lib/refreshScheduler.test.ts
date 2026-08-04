import { afterEach, describe, expect, it, vi } from "vitest";
import { KeyedRefreshScheduler } from "./refreshScheduler";

const flushMicrotasks = async () => {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
};

let scheduler: KeyedRefreshScheduler | null = null;

afterEach(() => {
  scheduler?.dispose();
  scheduler = null;
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("KeyedRefreshScheduler", () => {
  it("invalidates the previous timer generation before scheduling the next fallback", async () => {
    vi.useFakeTimers();
    const task = vi.fn(async () => undefined);
    scheduler = new KeyedRefreshScheduler();
    const refresh = scheduler.register("runtime", task, { fallbackDelayMs: () => 1_000 });

    await flushMicrotasks();
    expect(task).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(500);
    await refresh.invalidate();
    expect(task).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(500);
    expect(task).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(500);
    expect(task).toHaveBeenCalledTimes(3);
  });

  it("pauses fallback and event work while hidden, then performs one visibility refresh", async () => {
    vi.useFakeTimers();
    let visibility: DocumentVisibilityState = "visible";
    vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
    const task = vi.fn(async () => undefined);
    scheduler = new KeyedRefreshScheduler();
    const refresh = scheduler.register("connection", task, { fallbackDelayMs: () => 1_000 });
    await flushMicrotasks();
    expect(task).toHaveBeenCalledTimes(1);

    visibility = "hidden";
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(5_000);
    await refresh.invalidate();
    expect(task).toHaveBeenCalledTimes(1);

    await refresh.refresh();
    expect(task).toHaveBeenCalledTimes(2);

    visibility = "visible";
    document.dispatchEvent(new Event("visibilitychange"));
    document.dispatchEvent(new Event("visibilitychange"));
    await flushMicrotasks();
    expect(task).toHaveBeenCalledTimes(3);
  });

  it("coalesces manual and event refreshes into one in-flight task per key", async () => {
    vi.useFakeTimers();
    let resolveTask!: (value: string) => void;
    const pending = new Promise<string>((resolve) => { resolveTask = resolve; });
    const firstTask = vi.fn(() => pending);
    const secondTask = vi.fn(async () => "second");
    scheduler = new KeyedRefreshScheduler();
    const first = scheduler.register("shared", firstTask, { fallbackDelayMs: () => 1_000 });
    const second = scheduler.register("shared", secondTask, { fallbackDelayMs: () => 1_000 });
    await flushMicrotasks();
    expect(firstTask).toHaveBeenCalledTimes(1);
    expect(secondTask).not.toHaveBeenCalled();

    const manual = first.refresh();
    const event = second.invalidate();
    expect(manual).toBe(event);
    expect(firstTask).toHaveBeenCalledTimes(1);

    resolveTask("done");
    await Promise.all([manual, event]);
    await flushMicrotasks();
    expect(vi.getTimerCount()).toBe(1);

    first.release();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(secondTask).toHaveBeenCalledTimes(1);
  });

  it("removes fallback and visibility work after the last interest is released", async () => {
    vi.useFakeTimers();
    let visibility: DocumentVisibilityState = "visible";
    vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
    const task = vi.fn(async () => undefined);
    scheduler = new KeyedRefreshScheduler();
    const refresh = scheduler.register("cleanup", task, { fallbackDelayMs: () => 1_000 });
    await flushMicrotasks();
    expect(task).toHaveBeenCalledTimes(1);

    refresh.release();
    expect(vi.getTimerCount()).toBe(0);
    await vi.advanceTimersByTimeAsync(5_000);
    visibility = "hidden";
    document.dispatchEvent(new Event("visibilitychange"));
    visibility = "visible";
    document.dispatchEvent(new Event("visibilitychange"));
    await flushMicrotasks();
    expect(task).toHaveBeenCalledTimes(1);
  });
});
