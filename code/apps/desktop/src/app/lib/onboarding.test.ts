import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("./native", () => ({
  invokeNative: (...args: unknown[]) => mocks.invokeNative(...args),
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
}));

import {
  deriveOnboardingEvidence,
  parseOnboardingMarker,
  readOnboardingMarker,
  useOnboardingGate,
  writeOnboardingMarker,
} from "./onboarding";

afterEach(() => {
  cleanup();
  mocks.invokeNative.mockReset();
});

describe("parseOnboardingMarker", () => {
  it("returns null for empty, malformed, or non-object payloads", () => {
    expect(parseOnboardingMarker(null)).toBeNull();
    expect(parseOnboardingMarker({})).toBeNull();
    expect(parseOnboardingMarker({ onboarding: { completed: "yes" } })).toBeNull();
    expect(parseOnboardingMarker({ onboarding: 7 })).toBeNull();
  });

  it("parses a persisted completion marker", () => {
    const marker = parseOnboardingMarker({ onboarding: { completed: true, completed_at_ms: 123, mode: "user-confirmed" } });
    expect(marker).toEqual({ completed: true, completed_at_ms: 123, mode: "user-confirmed" });
  });
});

describe("readOnboardingMarker", () => {
  it("reads the marker from the preferences settings section", async () => {
    mocks.invokeNative.mockResolvedValue({
      state: "loaded",
      payload: { onboarding: { completed: true, completed_at_ms: 1, mode: "user-confirmed" } },
    });
    await expect(readOnboardingMarker()).resolves.toEqual({ completed: true, completed_at_ms: 1, mode: "user-confirmed" });
    expect(mocks.invokeNative).toHaveBeenCalledWith("get_app_settings", { section: "preferences" });
  });

  it("treats a missing settings file as no marker (first run without preferences)", async () => {
    mocks.invokeNative.mockResolvedValue({ state: "missing", payload: {} });
    await expect(readOnboardingMarker()).resolves.toBeNull();
  });

  it("rejects when the settings store is unreachable", async () => {
    mocks.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    await expect(readOnboardingMarker()).rejects.toThrow("native_unavailable");
  });
});

describe("writeOnboardingMarker", () => {
  it("persists the completion marker in the preferences section", async () => {
    mocks.invokeNative.mockResolvedValue({ ok: true, summary: "Settings saved" });
    await expect(writeOnboardingMarker("user-confirmed")).resolves.toBeUndefined();
    expect(mocks.invokeNative).toHaveBeenCalledWith("save_app_settings", {
      section: "preferences",
      payload: expect.objectContaining({
        onboarding: expect.objectContaining({ completed: true, mode: "user-confirmed" }),
        scope: "d-token-local-settings",
      }),
    });
  });

  it("rejects when the native write fails closed", async () => {
    mocks.invokeNative.mockResolvedValue({ ok: false, summary: "Settings save failed" });
    await expect(writeOnboardingMarker("user-confirmed")).rejects.toThrow("Settings save failed");
  });
});

describe("deriveOnboardingEvidence", () => {
  it("reports connected evidence only from native connected-state claims", () => {
    const snapshot = {
      agent_bindings: [
        { agent_id: "opencode", already_connected: true },
        { agent_id: "codex", already_connected: false },
      ],
      agent_route_follow: { enabled: false },
    };
    expect(deriveOnboardingEvidence(snapshot)).toEqual({
      connectedAgents: ["opencode"],
      routeFollowEnabled: false,
      hasConnectedEvidence: true,
    });
  });

  it("never treats mere detection as connection", () => {
    const snapshot = {
      agent_bindings: [
        { agent_id: "opencode", already_connected: false },
        { agent_id: "codex", already_connected: false },
      ],
      agent_route_follow: undefined,
    };
    expect(deriveOnboardingEvidence(snapshot).hasConnectedEvidence).toBe(false);
    expect(deriveOnboardingEvidence(snapshot).connectedAgents).toEqual([]);
  });

  it("accepts an enabled agent route follow as connected evidence", () => {
    const snapshot = { agent_bindings: [], agent_route_follow: { enabled: true } };
    expect(deriveOnboardingEvidence(snapshot).hasConnectedEvidence).toBe(true);
  });

  it("handles a null snapshot (discovery unavailable)", () => {
    expect(deriveOnboardingEvidence(null).hasConnectedEvidence).toBe(false);
  });
});

describe("useOnboardingGate", () => {
  it("starts checking and becomes pending when no marker exists (first run)", async () => {
    mocks.invokeNative.mockResolvedValue({ state: "missing", payload: {} });
    const { result } = renderHook(() => useOnboardingGate());
    expect(result.current.state).toBe("checking");
    await waitFor(() => expect(result.current.state).toBe("pending"));
  });

  it("becomes completed when the marker is persisted", async () => {
    mocks.invokeNative.mockResolvedValue({
      state: "loaded",
      payload: { onboarding: { completed: true, completed_at_ms: 1, mode: "user-confirmed" } },
    });
    const { result } = renderHook(() => useOnboardingGate());
    await waitFor(() => expect(result.current.state).toBe("completed"));
  });

  it("becomes unavailable when the settings store cannot be read", async () => {
    mocks.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    const { result } = renderHook(() => useOnboardingGate());
    await waitFor(() => expect(result.current.state).toBe("unavailable"));
  });

  it("complete() persists the marker and flips the gate to completed", async () => {
    mocks.invokeNative.mockResolvedValue({ state: "missing", payload: {} });
    const { result } = renderHook(() => useOnboardingGate());
    await waitFor(() => expect(result.current.state).toBe("pending"));

    mocks.invokeNative.mockResolvedValue({ ok: true, summary: "Settings saved" });
    let ok = false;
    await act(async () => {
      ok = await result.current.complete();
    });
    expect(ok).toBe(true);
    expect(result.current.state).toBe("completed");
    expect(mocks.invokeNative).toHaveBeenCalledWith("save_app_settings", expect.objectContaining({ section: "preferences" }));
  });

  it("complete() fails without flipping the gate when the write is rejected", async () => {
    mocks.invokeNative.mockResolvedValue({ state: "missing", payload: {} });
    const { result } = renderHook(() => useOnboardingGate());
    await waitFor(() => expect(result.current.state).toBe("pending"));

    mocks.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    let ok = true;
    await act(async () => {
      ok = await result.current.complete();
    });
    expect(ok).toBe(false);
    expect(result.current.state).toBe("pending");
  });
});
