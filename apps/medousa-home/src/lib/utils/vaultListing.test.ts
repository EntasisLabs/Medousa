import { describe, expect, it } from "vitest";
import {
  VAULT_LIST_MAX_PAGES,
  listingIncompleteAfterPages,
} from "./vaultListing";

describe("vaultListing", () => {
  it("flags an incomplete listing at the safety cap", () => {
    expect(listingIncompleteAfterPages(VAULT_LIST_MAX_PAGES, true, "cursor")).toBe(
      true,
    );
    expect(listingIncompleteAfterPages(1, true, "cursor")).toBe(false);
    expect(listingIncompleteAfterPages(VAULT_LIST_MAX_PAGES, false, undefined)).toBe(
      false,
    );
  });
});
