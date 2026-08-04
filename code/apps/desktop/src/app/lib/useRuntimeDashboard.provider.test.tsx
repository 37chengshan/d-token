import React from "react";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("./native", () => ({
  invokeNative: (...args: unknown[]) => mocks.invokeNative(...args),
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
}));

import {
  RuntimeEvidenceProvider,
  useRuntimeDashboard,
  useRuntimeDashboardDetail,
  useRuntimeDashboardSelector,
  type RuntimeDashboardState,
  type RuntimeDetailState,
} from "./useRuntimeDashboard";

const desktop = {
  source: "dtoken-owned-runtime",
  desktop_data_source: "dtoken-local-service",
  desktop_revision: "revision-1",
  desktop_collected_at: "2026-07-13T08:00:00.000Z",
  request_logs: [],
  providers: [],
  routes: [],
  instances: [],
};

const service = {
  state: "running",
  managed: true,
  pid: 42,
  manifest_identity_ok: true,
  executable_owned: true,
  port_owned: true,
};

afterEach(() => {
  cleanup();
  mocks.invokeNative.mockReset();
});

describe("RuntimeEvidenceProvider performance behavior", () => {
  it("does not rerender a slice subscriber when an unrelated service slice changes", async () => {
    let serviceStopped = false;
    mocks.invokeNative.mockImplementation(async (command: string) => command === "get_desktop_summary"
      ? { ...desktop }
      : { ...service, state: serviceStopped ? "stopped" : "running" });
    let latest: RuntimeDashboardState | null = null;
    let renders = 0;
    function RefreshProbe() {
      latest = useRuntimeDashboard();
      return null;
    }
    function StatusSliceProbe() {
      const status = useRuntimeDashboardSelector((state) => state.status);
      renders += 1;
      return <span>{status}</span>;
    }

    render(<RuntimeEvidenceProvider><RefreshProbe /><StatusSliceProbe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latest?.status).toBe("runtime"));
    const rendersAfterInitialLoad = renders;
    serviceStopped = true;
    await act(async () => { await latest!.refresh(); });

    expect(renders).toBe(rendersAfterInitialLoad);
    expect((latest as unknown as RuntimeDashboardState).operational.service.data?.state).toBe("stopped");
  });

  it("recomputes the selected value when only the selector changes", async () => {
    mocks.invokeNative.mockImplementation(async (command: string) => command === "get_desktop_summary" ? { ...desktop } : { ...service });
    let mode: "status" | "error" = "status";
    let observed = "";
    function Probe() {
      observed = useRuntimeDashboardSelector((state) => mode === "status" ? state.status : state.error);
      return <span>{observed}</span>;
    }

    const rendered = render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(observed).toBe("runtime"));

    mode = "error";
    rendered.rerender(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);

    expect(observed).toBe("");
  });

  it("maps an unversioned native desktop payload to an explicit partial state", async () => {
    mocks.invokeNative.mockImplementation(async (command: string) => command === "get_desktop_summary"
      ? { source: "dtoken-owned-runtime", desktop_data_source: "dtoken-local-service" }
      : { ...service });
    let latest: RuntimeDashboardState | null = null;
    function Probe() {
      latest = useRuntimeDashboard();
      return null;
    }

    render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latest?.status).toBe("partial"));
    const result = latest as unknown as RuntimeDashboardState;
    expect(result.error).toBe("desktop_payload_invalid");
    expect(result.payload).toBeNull();
  });

  it("deduplicates concurrent refreshes and does not rerender for the same native revision", async () => {
    mocks.invokeNative.mockImplementation(async (command: string) => command === "get_desktop_summary" ? { ...desktop } : { ...service });
    let latest: RuntimeDashboardState | null = null;
    let renders = 0;
    function Probe() {
      latest = useRuntimeDashboard();
      renders += 1;
      return <span>{latest.status}</span>;
    }

    render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latest?.status).toBe("runtime"));
    expect(mocks.invokeNative).toHaveBeenCalledTimes(2);
    const rendersAfterInitialLoad = renders;

    await act(async () => {
      await Promise.all([latest!.refresh(), latest!.refresh()]);
    });

    expect(mocks.invokeNative).toHaveBeenCalledTimes(4);
    expect(renders).toBe(rendersAfterInitialLoad);
  });

  it("publishes service liveness changes even when the desktop revision and PID are unchanged", async () => {
    let stale = false;
    mocks.invokeNative.mockImplementation(async (command: string) => command === "get_desktop_summary"
      ? { ...desktop }
      : {
          ...service,
          state: "stopped",
          pid_running: !stale,
          manifest_stale: stale,
        });
    let latest: RuntimeDashboardState | null = null;
    function Probe() {
      latest = useRuntimeDashboard();
      return null;
    }

    render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latest?.operational.service.data?.pid_running).toBe(true));
    stale = true;
    await act(async () => { await latest!.refresh(); });

    const result = latest as unknown as RuntimeDashboardState;
    expect(result.operational.service.data?.pid_running).toBe(false);
    expect(result.operational.service.data?.manifest_stale).toBe(true);
  });

  it("marks the last verified dashboard slice stale when one refresh loses only the desktop payload", async () => {
    let failDesktop = false;
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_summary") {
        if (failDesktop) throw new Error("temporary dashboard failure");
        return { ...desktop };
      }
      return { ...service };
    });
    let latest: RuntimeDashboardState | null = null;
    function Probe() {
      latest = useRuntimeDashboard();
      return null;
    }

    render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latest?.status).toBe("runtime"));
    failDesktop = true;
    await act(async () => { await latest!.refresh(); });

    const result = latest as unknown as RuntimeDashboardState;
    expect(result.status).toBe("stale");
    expect(result.payload?.desktop_revision).toBe("revision-1");
    expect(result.operational.dashboard.data?.desktop_revision).toBe("revision-1");
    expect(result.operational.dashboard.state).toBe("stale");
  });

  it("does not replace verified data with a fulfilled offline summary envelope", async () => {
    let offline = false;
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_summary") {
        return offline
          ? {
              source: "dtoken-desktop-control-plane",
              desktop_data_source: "dtoken-desktop-control-plane",
              desktop_revision: "offline-revision",
              evidence_state: "partial",
              complete: false,
              partial: true,
              request_logs: [],
              providers: [],
              routes: [],
              instances: [],
              error: "d-token local service is unavailable",
            }
          : { ...desktop, request_logs: [{ agent: "OpenCode" }] };
      }
      return { ...service };
    });
    let latest: RuntimeDashboardState | null = null;
    function Probe() {
      latest = useRuntimeDashboard();
      return null;
    }

    render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latest?.status).toBe("runtime"));
    offline = true;
    await act(async () => { await latest!.refresh(); });

    const result = latest as unknown as RuntimeDashboardState;
    expect(result.status).toBe("stale");
    expect(result.payload?.desktop_revision).toBe("revision-1");
    expect(result.payload?.request_logs).toEqual([{ agent: "OpenCode" }]);
    expect(result.operational.traffic.data).toEqual([{ agent: "OpenCode" }]);
    expect(result.operational.traffic.state).toBe("stale");
  });

  it("restores a persisted stale runtime snapshot on a fresh App lifecycle", async () => {
    mocks.invokeNative.mockImplementation(async (command: string) => command === "get_desktop_summary"
      ? {
          ...desktop,
          desktop_revision: "persisted-revision",
          evidence_state: "stale",
          freshness: "stale",
          complete: false,
          partial: true,
          runtime_available: false,
          stats: { requests: { total: 125, failed: 20 } },
          request_logs: [{ agent: "OpenCode" }],
          error: "d-token local service is unavailable",
        }
      : { ...service, state: "stopped", pid_running: false });
    let latest: RuntimeDashboardState | null = null;
    function Probe() {
      latest = useRuntimeDashboard();
      return null;
    }

    render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latest?.status).toBe("stale"));

    const result = latest as unknown as RuntimeDashboardState;
    expect(result.payload?.desktop_revision).toBe("persisted-revision");
    expect((result.payload?.stats as { requests?: { total?: number } } | undefined)?.requests?.total).toBe(125);
    expect(result.operational.traffic.data).toEqual([{ agent: "OpenCode" }]);
    expect(result.operational.traffic.state).toBe("stale");
    expect(result.updatedAt?.toISOString()).toBe("2026-07-13T08:00:00.000Z");
  });

  it("renders the native unix timestamp as a valid Date", async () => {
    mocks.invokeNative.mockImplementation(async (command: string) => command === "get_desktop_summary"
      ? { ...desktop, desktop_collected_at: "unix:1783929600" }
      : { ...service });
    let latest: RuntimeDashboardState | null = null;
    function Probe() {
      latest = useRuntimeDashboard();
      return null;
    }

    render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latest?.status).toBe("runtime"));

    const result = latest as unknown as RuntimeDashboardState;
    expect(Number.isFinite(result.updatedAt?.getTime())).toBe(true);
    expect(result.updatedAt?.toISOString()).toBe("2026-07-13T08:00:00.000Z");
  });

  it("loads the full desktop payload only after a detail consumer mounts", async () => {
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_summary") return { ...desktop };
      if (command === "get_desktop_data") return { ...desktop, desktop_payload_kind: "detail" };
      return { ...service };
    });
    let showDetail = false;
    function Probe() {
      useRuntimeDashboard();
      if (showDetail) return <DetailProbe />;
      return null;
    }
    function DetailProbe() {
      useRuntimeDashboardDetail();
      return null;
    }
    const rendered = render(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(mocks.invokeNative).toHaveBeenCalledWith("get_desktop_summary"));
    expect(mocks.invokeNative).not.toHaveBeenCalledWith("get_desktop_data");

    showDetail = true;
    rendered.rerender(<RuntimeEvidenceProvider><Probe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(mocks.invokeNative).toHaveBeenCalledWith("get_desktop_data"));
  });

  it("retains the last verified detail payload when a fulfilled refresh becomes partial", async () => {
    let detailUnavailable = false;
    mocks.invokeNative.mockImplementation(async (command: string) => {
      if (command === "get_desktop_summary") return { ...desktop };
      if (command === "get_desktop_data") {
        return detailUnavailable
          ? {
              ...desktop,
              desktop_revision: "detail-unavailable",
              source: "dtoken-desktop-control-plane",
              desktop_data_source: "dtoken-desktop-control-plane",
              partial: true,
              request_logs: [],
              error: "detail service unavailable",
            }
          : { ...desktop, desktop_payload_kind: "detail", request_logs: [{ agent: "Codex" }] };
      }
      return { ...service };
    });
    let latestDetail: RuntimeDetailState | null = null;
    function DetailProbe() {
      latestDetail = useRuntimeDashboardDetail();
      return null;
    }

    render(<RuntimeEvidenceProvider><DetailProbe /></RuntimeEvidenceProvider>);
    await waitFor(() => expect(latestDetail?.status).toBe("runtime"));
    detailUnavailable = true;
    await act(async () => { await latestDetail!.refresh(); });

    const result = latestDetail as unknown as RuntimeDetailState;
    expect(result.status).toBe("stale");
    expect(result.payload?.desktop_revision).toBe("revision-1");
    expect(result.payload?.request_logs).toEqual([{ agent: "Codex" }]);
    expect(result.error).toBe("RUNTIME_UNAVAILABLE");
  });
});
