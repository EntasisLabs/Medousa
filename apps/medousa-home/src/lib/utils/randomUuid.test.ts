import { describe, expect, it, vi } from "vitest";
import { randomUuid } from "$lib/utils/randomUuid";

describe("randomUuid", () => {
  it("uses the native implementation when the WebView provides it", () => {
    const native = "11111111-2222-4333-8444-555555555555";
    const randomUUID = vi.fn(() => native);

    expect(randomUuid({ randomUUID })).toBe(native);
    expect(randomUUID).toHaveBeenCalledOnce();
  });

  it("builds an RFC 4122 v4 UUID from getRandomValues on older WebViews", () => {
    const getRandomValues = vi.fn((bytes: Uint8Array) => {
      bytes.fill(0);
      return bytes;
    });

    expect(randomUuid({ getRandomValues })).toBe(
      "00000000-0000-4000-8000-000000000000",
    );
    expect(getRandomValues).toHaveBeenCalledOnce();
  });
});
