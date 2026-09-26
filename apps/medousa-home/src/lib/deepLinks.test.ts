import { describe, expect, it } from "vitest";
import {
  parseDeepLink,
  undertakingLocationDeepLinkUrl,
  vaultDeepLinkUrl,
  workDeepLinkUrl,
} from "./deepLinks";

describe("deepLinks", () => {
  it("validates Live launch identity and conversation mode", () => {
    const requestId = "550e8400-e29b-41d4-a716-446655440000";
    for (const mode of ["new", "resume"]) {
      expect(parseDeepLink(`medousa://live?mode=${mode}&request=${requestId}`))
        .toEqual({ kind: "live", mode, requestId });
    }
    for (const url of [
      "medousa://live?mode=new&request=invalid",
      `medousa://live?mode=unknown&request=${requestId}`,
      `medousa://live?request=${requestId}`,
      `medousa://live/other?mode=new&request=${requestId}`,
    ]) expect(parseDeepLink(url)).toBeNull();
  });
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

  it("parses Ask Medousa deeplinks", () => {
    expect(parseDeepLink("medousa://ask?request=550e8400-e29b-41d4-a716-446655440000")).toEqual({
      kind: "ask",
      requestId: "550e8400-e29b-41d4-a716-446655440000",
    });
    expect(parseDeepLink("medousa://ask?request=not-a-receipt")).toBe(null);
  });

  it("parses widget composer actions", () => {
    const request = "550e8400-e29b-41d4-a716-446655440000";
    for (const action of ["new", "camera", "notes", "photos", "calendar", "projects"] as const) {
      expect(parseDeepLink(`medousa://compose?action=${action}&request=${request}`)).toEqual({
        kind: "compose", action, requestId: request,
      });
    }
    expect(parseDeepLink(`medousa://compose?action=files&request=${request}`)).toBeNull();
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
});
