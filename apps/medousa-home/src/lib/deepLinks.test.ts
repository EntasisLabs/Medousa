import { describe, expect, it } from "vitest";
import {
  parseDeepLink,
  undertakingLocationDeepLinkUrl,
  vaultDeepLinkUrl,
  workDeepLinkUrl,
} from "./deepLinks";

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

  it("round-trips undertaking locations", () => {
    const url = undertakingLocationDeepLinkUrl({
      workId: "work-1",
      path: "src/lib.rs",
      line: 42,
      entityId: "function:run",
    });
    expect(parseDeepLink(url)).toEqual({
      kind: "undertaking_location",
      workId: "work-1",
      path: "src/lib.rs",
      line: 42,
      entityId: "function:run",
    });
    expect(parseDeepLink("medousa://undertaking/work-1/location?path=../secret"))
      .toBe(null);
    expect(parseDeepLink("medousa://undertaking/work-1/location?path=%2Fetc%2Fpasswd"))
      .toBe(null);
  });

  it("recognizes MCP OAuth callbacks", () => {
    const callback = "medousa://mcp/oauth/callback?code=abc&state=xyz";
    expect(parseDeepLink(callback)).toEqual({
      kind: "mcp_oauth_callback",
      callbackUrl: callback,
    });
  });
});
