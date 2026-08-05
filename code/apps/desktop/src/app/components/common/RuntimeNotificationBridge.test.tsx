import { describe, expect, it } from "vitest";
import { autoUpdateNotification, observableServiceState } from "./RuntimeNotificationBridge";

describe("observableServiceState", () => {
  it("normalizes managed and unmanaged running instances", () => {
    expect(observableServiceState("running", "verified")).toBe("running");
    expect(observableServiceState("unmanaged_running", "verified")).toBe("running");
  });

  it("does not turn loading into a false outage", () => {
    expect(observableServiceState("", "loading")).toBeNull();
    expect(observableServiceState("", "unavailable")).toBe("unavailable");
  });

  it("keeps a verified runtime summary from reporting a transient service probe outage", () => {
    expect(observableServiceState("", "unavailable", "runtime")).toBe("running");
  });
});

describe("autoUpdateNotification", () => {
  it("only promotes a cryptographically verified update", () => {
    expect(autoUpdateNotification({ ok: true, state: "update_available", signature_verified: true, latest_version: "0.3.0" }, "stable", "zh").level).toBe("action-required");
    expect(autoUpdateNotification({ ok: true, state: "update_available", signature_verified: false, latest_version: "0.3.0" }, "stable", "zh").level).toBe("warning");
  });

  it("keeps an unavailable feed visible without claiming an update", () => {
    const notification = autoUpdateNotification({ ok: false, state: "unavailable", summary: "feed unavailable" }, "preview", "en");
    expect(notification.title).toBe("Signed update feed unavailable");
    expect(notification.receipt.targetId).toBe("update:preview");
  });
});
