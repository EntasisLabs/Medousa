import { describe, expect, it } from "vitest";
import {
  artifactStoreScopeId,
  isMedousaStoreRequest,
  isValidStoreKey,
} from "./medousaStoreClient";

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

  it("derives stable kebab-case store scopes from artifact ids", () => {
    const a = artifactStoreScopeId("art:sess:ui:abc123");
    const b = artifactStoreScopeId("art:sess:ui:abc123");
    const c = artifactStoreScopeId("art:other");
    expect(a).toBe(b);
    expect(a).toMatch(/^art-kv-[0-9a-f]{12}$/);
    expect(a).not.toBe(c);
  });
});
