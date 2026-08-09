import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { StepProgress } from "./StepProgress";

afterEach(cleanup);

const steps = [
  { key: "check", label: "检查" },
  { key: "backup", label: "备份" },
  { key: "start", label: "启动" },
  { key: "verify", label: "验证" },
];

describe("StepProgress", () => {
  it("renders every phase label and pending numbers when nothing is running", () => {
    const { container } = render(<StepProgress steps={steps} current={-1} done={false} />);
    for (const label of ["检查", "备份", "启动", "验证"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    for (const number of ["1", "2", "3", "4"]) {
      expect(screen.getByText(number)).toBeInTheDocument();
    }
    expect(container.querySelector(".lucide-check")).toBeNull();
    expect(container.querySelector(".animate-spin")).toBeNull();
    expect(container.querySelector(".animate-pulse")).toBeNull();
  });

  it("marks completed steps with check icons", () => {
    const { container } = render(<StepProgress steps={steps} current={2} done={false} />);
    expect(container.querySelectorAll(".lucide-check")).toHaveLength(2);
  });

  it("shows an indeterminate breathing marker when the exact stage is unknown", () => {
    const { container } = render(<StepProgress steps={steps} current={0} done={false} indeterminate />);
    expect(container.querySelector('[data-progress-indeterminate="true"]')).toBeInTheDocument();
    const dot = container.querySelector('[data-progress-indeterminate-dot="true"]');
    expect(dot).not.toBeNull();
    expect(dot).toHaveClass("animate-pulse");
  });

  it("keeps the indeterminate marker static under reduced motion", () => {
    const { container } = render(<StepProgress steps={steps} current={0} done={false} indeterminate reducedMotion />);
    expect(container.querySelector('[data-reduced-motion="true"]')).toBeInTheDocument();
    const dot = container.querySelector('[data-progress-indeterminate-dot="true"]');
    expect(dot).not.toBeNull();
    expect(dot).not.toHaveClass("animate-pulse");
    for (const label of ["检查", "备份", "启动", "验证"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  it("disables spin and check animations for the active step under reduced motion", () => {
    const { container } = render(<StepProgress steps={steps} current={1} done={false} reducedMotion />);
    expect(container.querySelector(".animate-spin")).toBeNull();
    expect(container.querySelector(".animate-pulse")).toBeNull();
  });

  it("mirrors the done state on the root when every phase completed", () => {
    const { container } = render(<StepProgress steps={steps} current={0} done />);
    expect(container.querySelector('[data-progress-done="true"]')).toBeInTheDocument();
    expect(container.querySelectorAll(".lucide-check")).toHaveLength(steps.length);
  });
});
