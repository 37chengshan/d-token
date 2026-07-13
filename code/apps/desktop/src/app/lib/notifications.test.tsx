import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { NotificationProvider, useNotifications } from "./notifications";
import { NotificationBanner } from "../components/common/NotificationCenter";

function Harness() {
  const store = useNotifications();
  return (
    <div>
      <NotificationBanner />
      <output data-testid="count">{store.notifications.length}</output>
      <output data-testid="unread">{store.unreadCount}</output>
      <output data-testid="occurrences">{store.notifications[0]?.count ?? 0}</output>
      <output data-testid="message">{store.notifications[0]?.message ?? ""}</output>
      <button type="button" onClick={() => store.addNotification({ dedupeKey: "doctor", level: "warning", title: "Doctor", message: "Needs attention" })}>warning</button>
      <button type="button" onClick={() => store.addNotification({ dedupeKey: "save", level: "success", title: "Saved", message: "sk-secret-value" })}>success</button>
      <button type="button" onClick={() => store.addNotification({ dedupeKey: "receipt", level: "success", title: "Receipt", message: "Completed", persistent: true })}>persistent success</button>
      <button type="button" onClick={store.markAllRead}>read</button>
    </div>
  );
}

beforeEach(() => window.localStorage.clear());
afterEach(cleanup);

describe("NotificationProvider", () => {
  it("deduplicates repeated events and keeps persistent warnings across restart", async () => {
    const user = userEvent.setup();
    const first = render(<NotificationProvider><Harness /></NotificationProvider>);
    await user.click(screen.getByRole("button", { name: "warning" }));
    await user.click(screen.getByRole("button", { name: "warning" }));
    expect(screen.getByTestId("count")).toHaveTextContent("1");
    expect(screen.getByTestId("occurrences")).toHaveTextContent("2");
    expect(screen.getByTestId("unread")).toHaveTextContent("1");
    first.unmount();

    render(<NotificationProvider><Harness /></NotificationProvider>);
    expect(screen.getByTestId("count")).toHaveTextContent("1");
    expect(screen.getByTestId("occurrences")).toHaveTextContent("2");
  });

  it("redacts notification text and does not persist ordinary success", async () => {
    const user = userEvent.setup();
    const first = render(<NotificationProvider><Harness /></NotificationProvider>);
    await user.click(screen.getByRole("button", { name: "success" }));
    expect(screen.getByTestId("message")).toHaveTextContent("[REDACTED]");
    first.unmount();
    render(<NotificationProvider><Harness /></NotificationProvider>);
    expect(screen.getByTestId("count")).toHaveTextContent("0");
  });

  it("keeps persistent success receipts in the center without showing a banner", async () => {
    const user = userEvent.setup();
    const { container } = render(<NotificationProvider><Harness /></NotificationProvider>);
    await user.click(screen.getByRole("button", { name: "persistent success" }));
    expect(screen.getByTestId("count")).toHaveTextContent("1");
    expect(container.querySelector(".notification-banner")).toBeNull();

    await user.click(screen.getByRole("button", { name: "warning" }));
    expect(container.querySelector(".notification-banner")).not.toBeNull();
  });

  it("drops schema-less development v1 notifications and writes a v2 envelope", async () => {
    window.localStorage.setItem("d-token.notifications.v1", JSON.stringify([{
      id: "legacy",
      dedupeKey: "rollback-legacy",
      level: "success",
      title: "配置已回退",
      message: "d-token active config restored from verified backup",
      timestamp: 1,
      read: false,
      persistent: true,
      count: 1,
      receipt: { requestId: "rollback:legacy", rollbackId: "legacy" },
    }]));

    render(<NotificationProvider><Harness /></NotificationProvider>);

    expect(screen.getByTestId("count")).toHaveTextContent("0");
    await waitFor(() => expect(window.localStorage.getItem("d-token.notifications.v1")).toBeNull());
    await waitFor(() => expect(JSON.parse(window.localStorage.getItem("d-token.notifications.v2") ?? "null")).toEqual({
      schemaVersion: 2,
      notifications: [],
    }));
  });
});
