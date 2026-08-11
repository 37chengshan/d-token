import React from "react";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("./native", () => ({
  invokeNative: (...args: unknown[]) => mocks.invokeNative(...args),
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
}));

import { ConnectionSetupProvider, useConnectionSetup } from "./connectionSetup";

const loaded = {
  source: "dtoken-desktop-read-only-discovery",
  freshness: "unix:1",
  revision: "setup-1",
  state: "loaded",
  candidates: [],
  agent_bindings: [{
    agent_id: "opencode",
    agent_name: "OpenCode",
    installed: true,
    current_config_source: "agent-config:opencode",
    candidates: [],
    resolution: "already-connected",
    confidence: 100,
    reasons: [],
    capability: "one-click",
    already_connected: true,
  }],
  sources: [],
  has_resolution_gap: false,
  cc_switch_sync: null,
  writes_performed: false,
  privacy: "metadata only",
};

afterEach(() => {
  cleanup();
  mocks.invokeNative.mockReset();
});

describe("ConnectionSetupProvider refresh behavior", () => {
  it("loads discovery at app scope and refreshes immediately after a runtime invalidation event", async () => {
    mocks.invokeNative.mockResolvedValue(loaded);
    let latest: ReturnType<typeof useConnectionSetup> | null = null;
    function Probe() {
      latest = useConnectionSetup();
      return null;
    }

    render(<ConnectionSetupProvider><Probe /></ConnectionSetupProvider>);
    await waitFor(() => expect(latest?.status).toBe("ready"));
    expect(mocks.invokeNative).toHaveBeenCalledWith("discover_upstream_candidates");

    await act(async () => {
      window.dispatchEvent(new CustomEvent("dtoken:runtime-invalidated"));
    });
    await waitFor(() => expect(mocks.invokeNative).toHaveBeenCalledTimes(2));
  });

  it("retains the last discovery snapshot as stale when a fulfilled refresh is unavailable", async () => {
    let unavailable = false;
    mocks.invokeNative.mockImplementation(async () => unavailable
      ? {
          ...loaded,
          revision: "unavailable",
          state: "unavailable",
          agent_bindings: [],
          error: "discovery worker failed",
        }
      : loaded);
    let latest: ReturnType<typeof useConnectionSetup> | null = null;
    function Probe() {
      latest = useConnectionSetup();
      return null;
    }

    render(<ConnectionSetupProvider><Probe /></ConnectionSetupProvider>);
    await waitFor(() => expect(latest?.status).toBe("ready"));
    unavailable = true;
    await act(async () => {
      window.dispatchEvent(new CustomEvent("dtoken:runtime-invalidated"));
    });
    await waitFor(() => expect(latest?.status).toBe("stale"));

    const current = latest as unknown as ReturnType<typeof useConnectionSetup>;
    expect(current.snapshot?.revision).toBe("setup-1");
    expect(current.snapshot?.agent_bindings[0]?.agent_name).toBe("OpenCode");
    expect(current.error).toBe("connection_setup_unavailable");
  });
});
