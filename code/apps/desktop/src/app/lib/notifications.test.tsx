import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationProvider, useNotifications } from "./notifications";
import { NotificationBanner, NotificationCenter } from "../components/common/NotificationCenter";

function Harness() {
  const store = useNotifications();
  const [returnedId, setReturnedId] = useState("");
  return (
    <div>
      <NotificationBanner />
      <NotificationCenter />
      <output data-testid="count">{store.notifications.length}</output>
      <output data-testid="unread">{store.unreadCount}</output>
      <output data-testid="occurrences">{store.notifications[0]?.count ?? 0}</output>
      <output data-testid="message">{store.notifications[0]?.message ?? ""}</output>
      <output data-testid="details">{store.notifications[0]?.details ?? ""}</output>
      <output data-testid="receipt">{store.notifications[0]?.receipt?.requestId ?? ""}</output>
      <output data-testid="returned-id">{returnedId}</output>
      <output data-testid="stored-id">{store.notifications[0]?.id}</output>
      <button type="button" onClick={() => store.addNotification({ dedupeKey: "doctor", level: "warning", title: "Doctor", message: "Needs attention" })}>warning</button>
      <button type="button" onClick={() => store.addNotification({ dedupeKey: "save", level: "success", title: "Saved", message: "sk-secret-value" })}>success</button>
      <button type="button" onClick={() => store.addNotification({ dedupeKey: "receipt", level: "success", title: "Receipt", message: "Completed", persistent: true })}>persistent success</button>
      <button type="button" onClick={() => store.addNotification({ dedupeKey: "long-error", level: "error", title: "Failure", message: "x".repeat(400), details: "y".repeat(600) })}>long error</button>
      <button type="button" onClick={() => setReturnedId(store.addNotification({ dedupeKey: "retry", level: "error", title: "Failed", message: "Retry this operation", details: "Previous failure", receipt: { requestId: "previous-attempt" } }))}>failed attempt</button>
      <button type="button" onClick={() => setReturnedId(store.addNotification({ dedupeKey: "retry", level: "success", title: "Done", message: "Completed" }))}>retry</button>
      <button type="button" onClick={() => {
        for (let i = 0; i < 5; i++) store.addNotification({ dedupeKey: `event-${i}`, title: `Event ${i}`, level: "info", message: "Completed" });
      }}>burst</button>
      <button type="button" onClick={store.markAllRead}>read</button>
    </div>
  );
}

beforeEach(() => window.localStorage.clear());
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

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

  it("shows operation receipts outside the page without losing concurrent feedback", async () => {
    const user = userEvent.setup();
    const { container } = render(<NotificationProvider><Harness /></NotificationProvider>);
    await user.click(screen.getByRole("button", { name: "persistent success" }));
    expect(screen.getByTestId("count")).toHaveTextContent("1");
    expect(container.querySelector(".notification-toasts")).toBeNull();
    expect(document.querySelectorAll(".notification-toast")).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "warning" }));
    expect(document.querySelectorAll(".notification-toast")).toHaveLength(2);
  });

  it("closes success and error toasts after two seconds and retains unread history", () => {
    vi.useFakeTimers();
    render(<NotificationProvider><Harness /></NotificationProvider>);
    fireEvent.click(screen.getByRole("button", { name: "persistent success" }));
    fireEvent.click(screen.getByRole("button", { name: "long error" }));
    act(() => vi.advanceTimersByTime(1999));
    expect(document.querySelectorAll('.notification-toast[data-state="open"]')).toHaveLength(2);
    act(() => vi.advanceTimersByTime(1));
    expect(document.querySelectorAll('.notification-toast[data-state="closing"]')).toHaveLength(2);
    act(() => vi.advanceTimersByTime(160));
    expect(document.querySelector(".notification-toast")).toBeNull();
    expect(screen.getByTestId("count")).toHaveTextContent("2");
    expect(screen.getByTestId("unread")).toHaveTextContent("2");
  });

  it("closing a toast does not delete the operation record", () => {
    render(<NotificationProvider><Harness /></NotificationProvider>);
    fireEvent.click(screen.getByRole("button", { name: "warning" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭提示" }));
    expect(document.querySelector(".notification-toast")).toBeNull();
    expect(screen.getByTestId("count")).toHaveTextContent("1");
  });

  it("does not replay persisted notification history after restart", () => {
    const first = render(<NotificationProvider><Harness /></NotificationProvider>);
    fireEvent.click(screen.getByRole("button", { name: "warning" }));
    expect(document.querySelector(".notification-toast")).not.toBeNull();
    first.unmount();

    render(<NotificationProvider><Harness /></NotificationProvider>);
    expect(screen.getByTestId("count")).toHaveTextContent("1");
    expect(document.querySelector(".notification-toast")).toBeNull();
  });

  it("pauses for hover and keyboard focus, then resumes only the remaining time", () => {
    vi.useFakeTimers();
    render(<NotificationProvider><Harness /></NotificationProvider>);
    fireEvent.click(screen.getByRole("button", { name: "warning" }));
    const toast = document.querySelector(".notification-toast")!;
    act(() => vi.advanceTimersByTime(700));
    fireEvent.mouseEnter(toast);
    act(() => vi.advanceTimersByTime(10000));
    fireEvent.focus(screen.getByRole("button", { name: "查看" }));
    fireEvent.mouseLeave(toast);
    act(() => vi.advanceTimersByTime(10000));
    expect(toast).toHaveAttribute("data-state", "open");
    fireEvent.blur(screen.getByRole("button", { name: "查看" }), { relatedTarget: screen.getByRole("button", { name: "warning" }) });
    act(() => vi.advanceTimersByTime(1299));
    expect(toast).toHaveAttribute("data-state", "open");
    act(() => vi.advanceTimersByTime(1));
    expect(toast).toHaveAttribute("data-state", "closing");
    act(() => vi.advanceTimersByTime(160));
    expect(toast).not.toBeInTheDocument();
  });

  it("opens the selected notification details and marks only that record read", async () => {
    const user = userEvent.setup();
    render(<NotificationProvider><Harness /></NotificationProvider>);
    await user.click(screen.getByRole("button", { name: "failed attempt" }));
    await user.click(screen.getByRole("button", { name: "warning" }));
    const failedToast = Array.from(document.querySelectorAll<HTMLElement>(".notification-toast")).find((item) => item.textContent?.includes("Failed"))!;
    await user.click(within(failedToast).getByRole("button", { name: "查看" }));
    const center = screen.getByRole("dialog", { name: "通知中心" });
    expect(within(center).getByText("Previous failure")).toBeVisible();
    expect(within(center).getByText("previous-attempt")).toBeVisible();
    expect(screen.getByTestId("unread")).toHaveTextContent("1");
  });

  it("deduplicates burst events with a stable returned id and clears obsolete recovery details", () => {
    render(<NotificationProvider><Harness /></NotificationProvider>);
    fireEvent.click(screen.getByRole("button", { name: "failed attempt" }));
    const id = screen.getByTestId("returned-id").textContent;
    fireEvent.click(screen.getByRole("button", { name: "retry" }));
    expect(screen.getByTestId("returned-id").textContent).toBe(id);
    expect(screen.getByTestId("stored-id").textContent).toBe(id);
    expect(screen.getByTestId("details")).toBeEmptyDOMElement();
    expect(screen.getByTestId("receipt")).toBeEmptyDOMElement();
    expect(document.querySelectorAll(".notification-toast")).toHaveLength(1);
    expect(document.querySelector(".notification-toast")).toHaveTextContent("Done");
    fireEvent.click(screen.getByRole("button", { name: "burst" }));
    expect(document.querySelectorAll(".notification-toast")).toHaveLength(3);
    expect(screen.getByTestId("count")).toHaveTextContent("6");
  });

  it("limits notification summaries and details globally", () => {
    render(<NotificationProvider><Harness /></NotificationProvider>);
    fireEvent.click(screen.getByRole("button", { name: "long error" }));

    expect(screen.getByTestId("message").textContent?.length).toBeLessThanOrEqual(280);
    expect(screen.getByTestId("details").textContent?.length).toBeLessThanOrEqual(400);
  });

  it("ignores malformed persisted details and redacts valid persisted records without replaying them", () => {
    const record = { id: "stored", dedupeKey: "stored", title: "Stored", level: "warning", message: "sk-secret-value", timestamp: 1, read: false, persistent: true, count: 1 };
    window.localStorage.setItem("d-token.notifications.v2", JSON.stringify({ schemaVersion: 2, notifications: [
      { ...record, id: "broken-details", details: { error: "bad" } },
      { ...record, id: "broken-receipt", receipt: { requestId: {} } },
      record,
    ] }));
    render(<NotificationProvider><Harness /></NotificationProvider>);
    expect(screen.getByTestId("count")).toHaveTextContent("1");
    expect(screen.getByTestId("message")).toHaveTextContent("[REDACTED]");
    expect(document.querySelector(".notification-toast")).toBeNull();
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
