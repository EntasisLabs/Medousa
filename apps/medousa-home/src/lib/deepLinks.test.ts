import { describe, expect, it } from "vitest";
import { parseDeepLink, vaultDeepLinkUrl, workDeepLinkUrl } from "./deepLinks";

describe("deepLinks", () => {
  it("builds work and vault urls", () => {
    expect(workDeepLinkUrl("card-1")).toBe("medousa://work/card-1");
    expect(vaultDeepLinkUrl("journal/daily.md")).toBe("medousa://vault/journal%2Fdaily.md");
  });

  it("parses vault deeplinks", () => {
    expect(parseDeepLink("medousa://vault/journal/daily.md")).toEqual({
      kind: "vault",
      notePath: "journal/daily.md",
    });
    expect(parseDeepLink("medousa://vault/journal%2Fdaily.md")).toEqual({
      kind: "vault",
      notePath: "journal/daily.md",
    });
    expect(parseDeepLink("medousa://vault/../x.md")).toBe(null);
  });

  it("parses work deeplinks", () => {
    expect(parseDeepLink("medousa://work/card-1")).toEqual({
      kind: "work",
      cardId: "card-1",
    });
  });
});
