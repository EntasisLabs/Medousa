import { describe, expect, it } from "vitest";
import { isMedousaStoreRequest, isValidStoreKey } from "./medousaStoreClient";

describe("medousaStoreClient", () => {
  it("recognizes store postMessages", () => {
    expect(
      isMedousaStoreRequest({
        type: "medousa:store:set",
        requestId: "ms-1",
        key: "thoughts",
        value: [],
      }),
    ).toBe(true);
    expect(isMedousaStoreRequest({ type: "other", requestId: "x" })).toBe(false);
  });

  it("validates store keys", () => {
    expect(isValidStoreKey("thoughts")).toBe(true);
    expect(isValidStoreKey("bad key")).toBe(false);
  });
});
