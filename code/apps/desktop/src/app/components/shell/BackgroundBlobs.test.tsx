import "@testing-library/jest-dom/vitest";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { BackgroundBlobs } from "./BackgroundBlobs";

type ReducedMotionControl = {
  query: MediaQueryList;
  setMatches: (matches: boolean) => void;
};

const frameCallbacks = new Map<number, FrameRequestCallback>();
let nextFrameId = 1;
let context: CanvasRenderingContext2D;
let resizeDisconnect: ReturnType<typeof vi.fn>;

function installReducedMotion(initialMatches: boolean): ReducedMotionControl {
  let matches = initialMatches;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const query = {
    get matches() {
      return matches;
    },
    media: "(prefers-reduced-motion: reduce)",
    onchange: null,
    addEventListener: vi.fn((_type: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.add(listener);
    }),
    removeEventListener: vi.fn((_type: string, listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener);
    }),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  } as unknown as MediaQueryList;

  vi.stubGlobal("matchMedia", vi.fn(() => query));

  return {
    query,
    setMatches(nextMatches) {
      matches = nextMatches;
      const event = { matches, media: query.media } as MediaQueryListEvent;
      listeners.forEach((listener) => listener(event));
    },
  };
}

function runAnimationFrame(timestamp: number) {
  const callbacks = [...frameCallbacks.values()];
  frameCallbacks.clear();
  act(() => {
    callbacks.forEach((callback) => callback(timestamp));
  });
}

beforeEach(() => {
  frameCallbacks.clear();
  nextFrameId = 1;

  context = {
    clearRect: vi.fn(),
    fillStyle: "",
    fillText: vi.fn(),
    font: "",
    globalAlpha: 1,
    setTransform: vi.fn(),
    textAlign: "start",
    textBaseline: "alphabetic",
  } as unknown as CanvasRenderingContext2D;

  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context);
  vi.spyOn(HTMLCanvasElement.prototype, "getBoundingClientRect").mockReturnValue({
    bottom: 780,
    height: 780,
    left: 0,
    right: 1180,
    top: 0,
    width: 1180,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
  vi.spyOn(performance, "now").mockReturnValue(0);
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
    const id = nextFrameId;
    nextFrameId += 1;
    frameCallbacks.set(id, callback);
    return id;
  });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation((id) => {
    frameCallbacks.delete(id);
  });

  resizeDisconnect = vi.fn();
  class ResizeObserverMock {
    disconnect = resizeDisconnect;
    observe = vi.fn();
    unobserve = vi.fn();
  }
  vi.stubGlobal("ResizeObserver", ResizeObserverMock);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  frameCallbacks.clear();
});

describe("BackgroundBlobs", () => {
  it("renders one decorative canvas and reads the ambient theme tokens", () => {
    installReducedMotion(false);
    const computedStyle = {
      colorScheme: "light",
      getPropertyValue: vi.fn((name: string) => {
        if (name === "--ambient-glyph") return "#645c54";
        if (name === "--ambient-accent") return "#b85f43";
        if (name === "--font-mono") return "ui-monospace";
        return "";
      }),
    } as unknown as CSSStyleDeclaration;
    vi.spyOn(window, "getComputedStyle").mockReturnValue(computedStyle);

    const { container } = render(<BackgroundBlobs />);
    const canvas = container.querySelector("canvas.background-atmosphere");

    expect(canvas).toHaveAttribute("aria-hidden", "true");
    expect(container.querySelectorAll("canvas")).toHaveLength(1);
    expect(container.querySelector(".background-stars")).not.toBeInTheDocument();
    expect(container.querySelector(".background-nebula")).not.toBeInTheDocument();
    expect(canvas).toHaveAttribute("width", "1180");
    expect(canvas).toHaveAttribute("height", "780");
    expect(context.fillText).toHaveBeenCalled();
    expect(computedStyle.getPropertyValue).toHaveBeenCalledWith("--ambient-glyph");
    expect(computedStyle.getPropertyValue).toHaveBeenCalledWith("--ambient-accent");
  });

  it("gates active drawing to ten frames per second", () => {
    installReducedMotion(false);
    render(<BackgroundBlobs />);

    expect(context.clearRect).toHaveBeenCalledTimes(1);
    runAnimationFrame(0);
    expect(context.clearRect).toHaveBeenCalledTimes(2);

    runAnimationFrame(50);
    expect(context.clearRect).toHaveBeenCalledTimes(2);

    runAnimationFrame(100);
    expect(context.clearRect).toHaveBeenCalledTimes(3);
  });

  it("keeps system reduced motion static with no animation frame", () => {
    const reducedMotion = installReducedMotion(true);
    render(<BackgroundBlobs />);

    expect(context.clearRect).toHaveBeenCalledTimes(1);
    expect(window.requestAnimationFrame).not.toHaveBeenCalled();

    act(() => reducedMotion.setMatches(false));
    expect(window.requestAnimationFrame).toHaveBeenCalledTimes(1);
    expect(frameCallbacks).toHaveLength(1);

    act(() => reducedMotion.setMatches(true));
    expect(window.cancelAnimationFrame).toHaveBeenCalled();
    expect(frameCallbacks).toHaveLength(0);
    expect(context.clearRect).toHaveBeenCalledTimes(2);
  });

  it("degrades to the six-fps cadence after three consecutive slow frames", () => {
    installReducedMotion(false);
    render(<BackgroundBlobs />);
    let performanceRead = 0;
    vi.mocked(performance.now).mockImplementation(() => {
      const value = performanceRead % 2 === 0 ? 0 : 5;
      performanceRead += 1;
      return value;
    });
    runAnimationFrame(0);
    runAnimationFrame(100);
    runAnimationFrame(200);
    const drawsAfterDegrade = vi.mocked(context.clearRect).mock.calls.length;
    vi.mocked(performance.now).mockReturnValue(0);

    runAnimationFrame(300);
    expect(context.clearRect).toHaveBeenCalledTimes(drawsAfterDegrade);

    runAnimationFrame(367);
    expect(context.clearRect).toHaveBeenCalledTimes(drawsAfterDegrade + 1);
  });

  it("cancels animation and disconnects observers on unmount", () => {
    const reducedMotion = installReducedMotion(false);
    const { unmount } = render(<BackgroundBlobs />);

    expect(frameCallbacks).toHaveLength(1);
    unmount();

    expect(window.cancelAnimationFrame).toHaveBeenCalled();
    expect(resizeDisconnect).toHaveBeenCalledOnce();
    expect(reducedMotion.query.removeEventListener).toHaveBeenCalledWith("change", expect.any(Function));
    expect(frameCallbacks).toHaveLength(0);
  });
});
