import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MetricStrip, Surface } from "./Surface";

describe("flat product surfaces", () => {
  it("renders a quiet primary surface without legacy glass decoration", () => {
    const { container } = render(<Surface strong>content</Surface>);

    expect(screen.getByText("content")).toHaveAttribute("data-surface", "primary");
    expect(container.querySelector(".glass-backdrop,.glass-edge,.glass,.glass-strong")).toBeNull();
  });

  it("keeps metric cells inside one bounded strip", () => {
    const { container } = render(
      <MetricStrip>
        <div>one</div>
        <div>two</div>
        <div>three</div>
        <div>four</div>
      </MetricStrip>,
    );

    const strip = container.querySelector('[data-surface="metric-strip"]');
    expect(strip).toHaveTextContent("onetwothreefour");
    expect(strip?.children).toHaveLength(4);
    expect(strip?.querySelector("[data-surface]")).toBeNull();
  });

  it("exposes stable DOM hooks for the page-level no-nesting and two-primary guard", () => {
    const { container } = render(
      <main data-flat-page>
        <Surface strong>service</Surface>
        <Surface strong>workspace</Surface>
        <Surface>details</Surface>
        <MetricStrip>
          <div>one</div>
          <div>two</div>
        </MetricStrip>
      </main>,
    );

    const page = container.querySelector("[data-flat-page]");
    expect(page?.querySelectorAll('[data-surface="primary"]')).toHaveLength(2);
    expect(page?.querySelector('[data-surface="primary"] [data-surface], [data-surface="section"] [data-surface]')).toBeNull();
    expect(page?.querySelector('[data-surface="metric-strip"] [data-surface]')).toBeNull();
  });
});
