import { describe, expect, it } from "vitest";

import {
  assertWorkspaceMode,
  unsupportedAttachedCheckout,
  usesAttachedCheckout,
} from "$lib/forgeWorkspace";

describe("Forge workspace placement", () => {
  it("keeps missing legacy fields on the isolated default", () => {
    expect(usesAttachedCheckout({})).toBe(false);
  });

  it("recognizes both the durable mode and environment compatibility shape", () => {
    expect(usesAttachedCheckout({ workspace_mode: "attached_checkout" })).toBe(true);
    expect(
      usesAttachedCheckout({ environment: { kind: "attached_checkout" } }),
    ).toBe(true);
  });

  it("refuses a silent attached-to-isolated fallback from an older daemon", () => {
    expect(() => assertWorkspaceMode({}, "attached_checkout")).toThrow(
      unsupportedAttachedCheckout().message,
    );
    expect(() => assertWorkspaceMode({}, "isolated")).not.toThrow();
  });
});
