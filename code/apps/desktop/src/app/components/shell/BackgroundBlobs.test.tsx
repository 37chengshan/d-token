import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { BackgroundBlobs } from "./BackgroundBlobs";

afterEach(cleanup);

describe("BackgroundBlobs", () => {
  it("keeps the atmospheric layers decorative and outside the accessibility tree", () => {
    const { container } = render(<BackgroundBlobs />);
    const atmosphere = container.querySelector(".background-atmosphere");

    expect(atmosphere).toHaveAttribute("aria-hidden", "true");
    expect(container.querySelectorAll(".background-nebula")).toHaveLength(3);
    expect(container.querySelector(".background-stars")).toBeInTheDocument();
  });
});
