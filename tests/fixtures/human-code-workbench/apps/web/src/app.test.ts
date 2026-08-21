import { describe, expect, it } from "vitest";

describe("fixture web app", () => {
  it("renders the workbench", () => {
    expect("workbench").toContain("work");
  });
});
