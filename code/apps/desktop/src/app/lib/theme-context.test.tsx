import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));
const nextTheme = vi.hoisted(() => ({ setTheme: vi.fn() }));

vi.mock("./native", () => ({ invokeNative: native.invokeNative }));
vi.mock("next-themes", () => ({
  ThemeProvider: ({ children }: { children: React.ReactNode }) => children,
  useTheme: () => ({ setTheme: nextTheme.setTheme }),
}));

import { AppProviders, useAppearance } from "./theme-context";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
}

function Harness() {
  const appearance = useAppearance();
  return (
    <div>
      <output data-testid="theme">{appearance.theme}</output>
      <output data-testid="state">{appearance.loadState}</output>
      <output data-testid="notice">{appearance.persistenceNotice}</output>
      <button type="button" onClick={() => appearance.setTheme("light")}>light</button>
      <button type="button" onClick={() => appearance.setTheme("dark")}>dark</button>
    </div>
  );
}

beforeEach(() => {
  window.localStorage.clear();
  document.documentElement.className = "";
  document.documentElement.removeAttribute("style");
  nextTheme.setTheme.mockReset();
  native.invokeNative.mockReset();
  vi.stubGlobal("matchMedia", vi.fn(() => ({
    matches: false,
    media: "(prefers-color-scheme: dark)",
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("AppearanceProvider", () => {
  it("does not let late native hydration overwrite a user light-theme choice", async () => {
    window.localStorage.setItem("d-token.appearance.v2", JSON.stringify({ theme: "dark" }));
    const hydration = deferred<{ state: string; payload: { appearance: { theme: string } } }>();
    native.invokeNative.mockImplementation((command: string) => {
      if (command === "get_app_settings") return hydration.promise;
      return Promise.resolve({ ok: true, summary: "saved", logs: [], tools: [] });
    });

    const user = userEvent.setup();
    render(<AppProviders><Harness /></AppProviders>);
    expect(screen.getByTestId("theme")).toHaveTextContent("dark");

    await user.click(screen.getByRole("button", { name: "light" }));
    expect(screen.getByTestId("theme")).toHaveTextContent("light");
    expect(document.documentElement).not.toHaveClass("dark");

    await act(async () => {
      hydration.resolve({ state: "loaded", payload: { appearance: { theme: "dark" } } });
      await hydration.promise;
    });

    expect(screen.getByTestId("theme")).toHaveTextContent("light");
    expect(document.documentElement).not.toHaveClass("dark");
    expect(window.localStorage.getItem("d-token.theme")).toBe("light");
  });

  it("serializes rapid light-dark-light saves and leaves the final state authoritative", async () => {
    const saves = [deferred<any>(), deferred<any>(), deferred<any>()];
    let saveIndex = 0;
    native.invokeNative.mockImplementation((command: string) => {
      if (command === "get_app_settings") return Promise.resolve({ state: "loaded", payload: { appearance: { theme: "system" } } });
      if (command === "save_app_settings") {
        const current = saves[saveIndex];
        saveIndex += 1;
        return current.promise;
      }
      throw new Error(`unexpected:${command}`);
    });

    const user = userEvent.setup();
    render(<AppProviders><Harness /></AppProviders>);
    await waitFor(() => expect(screen.getByTestId("state")).toHaveTextContent("loaded"));

    await user.click(screen.getByRole("button", { name: "light" }));
    await user.click(screen.getByRole("button", { name: "dark" }));
    await user.click(screen.getByRole("button", { name: "light" }));
    expect(screen.getByTestId("theme")).toHaveTextContent("light");

    await waitFor(() => expect(saveIndex).toBe(1));
    await act(async () => saves[0].resolve({ ok: true, summary: "old-light", logs: [], tools: [] }));
    await waitFor(() => expect(saveIndex).toBe(2));
    expect(screen.getByTestId("notice")).toBeEmptyDOMElement();
    await act(async () => saves[1].resolve({ ok: true, summary: "old-dark", logs: [], tools: [] }));
    await waitFor(() => expect(saveIndex).toBe(3));
    expect(screen.getByTestId("notice")).toBeEmptyDOMElement();
    await act(async () => saves[2].resolve({ ok: true, summary: "final-light", logs: [], tools: [] }));

    await waitFor(() => expect(screen.getByTestId("notice")).toHaveTextContent("saved"));
    const saveCalls = native.invokeNative.mock.calls.filter(([command]) => command === "save_app_settings");
    expect(saveCalls).toHaveLength(3);
    expect(saveCalls[saveCalls.length - 1]?.[1]?.payload.appearance.theme).toBe("light");
    expect(screen.getByTestId("theme")).toHaveTextContent("light");
    expect(document.documentElement).not.toHaveClass("dark");
  });

  it("keeps the selected accent after a native save failure without restoring removed flow state", async () => {
    native.invokeNative.mockImplementation((command: string) => {
      if (command === "get_app_settings") return Promise.resolve({ state: "loaded", payload: {} });
      return Promise.reject(new Error("disk_full"));
    });

    function CustomHarness() {
      const appearance = useAppearance();
      return <button type="button" onClick={() => appearance.setAccent("purple")}>purple</button>;
    }

    const user = userEvent.setup();
    render(<AppProviders><CustomHarness /></AppProviders>);
    await user.click(screen.getByRole("button", { name: "purple" }));

    expect(document.documentElement).toHaveAttribute("data-accent", "purple");
    expect(document.documentElement).not.toHaveAttribute("data-flow-palette");
    expect(document.documentElement).not.toHaveAttribute("data-flow-speed");
    expect(document.documentElement.style.getPropertyValue("--flow-color-a")).toBe("");
    await waitFor(() => expect(native.invokeNative).toHaveBeenCalledWith("save_app_settings", expect.any(Object)));
    expect(document.documentElement).toHaveAttribute("data-accent", "purple");
  });
});
