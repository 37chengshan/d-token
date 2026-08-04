import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { OperationDialog } from "./OperationDialog";

afterEach(cleanup);

describe("OperationDialog", () => {
  it("shows impact and requires an explicit confirm action", async () => {
    const confirm = vi.fn();
    const user = userEvent.setup();
    render(
      <OperationDialog
        open
        onOpenChange={vi.fn()}
        title="Restore configuration"
        summary="Restore the reviewed rollback point."
        impact="The current local configuration will be replaced and verified."
        confirmLabel="Confirm restore"
        cancelLabel="Cancel"
        risk="danger"
        onConfirm={confirm}
      />,
    );
    expect(screen.getByText("The current local configuration will be replaced and verified.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Confirm restore" }));
    expect(confirm).toHaveBeenCalledTimes(1);
  });
});
