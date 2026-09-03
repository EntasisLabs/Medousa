import { describe, expect, it } from "vitest";
import { scrollTopAfterHistoryPrepend } from "$lib/utils/chatScrollPosition";

describe("scrollTopAfterHistoryPrepend", () => {
  it("offsets the current viewport by the anchor's movement", () => {
    expect(scrollTopAfterHistoryPrepend(240, 1_600, 2_400)).toBe(1_040);
  });

  it("does not pull the viewport backward when height does not grow", () => {
    expect(scrollTopAfterHistoryPrepend(240, 1_600, 1_580)).toBe(240);
  });
});
