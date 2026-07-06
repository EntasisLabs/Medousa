import { describe, expect, it } from "vitest";
import {
  buildMedousaNavigateClientScript,
  isMedousaNavigateRequest,
  isSafeVaultNavigatePath,
  MEDOUSA_NAVIGATE_CLIENT_SCRIPT_ID,
  parseMedousaVaultHref,
} from "./medousaNavigateClient";

describe("medousaNavigateClient", () => {
  it("injects navigate bridge script with marker id", () => {
    const html = buildMedousaNavigateClientScript();
    expect(html).toContain(MEDOUSA_NAVIGATE_CLIENT_SCRIPT_ID);
    expect(html).toContain("medousa:navigate");
    expect(html).toContain("Medousa.openVaultNote");
    expect(html).toContain("medousa://vault/");
  });

  it("recognizes navigate postMessages", () => {
    expect(
      isMedousaNavigateRequest({
        type: "medousa:navigate",
        target: "vault",
        path: "journal/daily.md",
      }),
    ).toBe(true);
    expect(isMedousaNavigateRequest({ type: "other" })).toBe(false);
    expect(
      isMedousaNavigateRequest({
        type: "medousa:navigate",
        target: "work",
        path: "card-1",
      }),
    ).toBe(false);
  });

  it("validates safe vault paths", () => {
    expect(isSafeVaultNavigatePath("journal/daily.md")).toBe(true);
    expect(isSafeVaultNavigatePath("")).toBe(false);
    expect(isSafeVaultNavigatePath("../secrets.md")).toBe(false);
    expect(isSafeVaultNavigatePath("/absolute.md")).toBe(false);
  });

  it("parses medousa://vault/ hrefs", () => {
    expect(parseMedousaVaultHref("medousa://vault/journal/daily.md")).toBe("journal/daily.md");
    expect(parseMedousaVaultHref("medousa://vault/journal%2Fdaily.md")).toBe("journal/daily.md");
    expect(parseMedousaVaultHref("medousa://work/card-1")).toBe(null);
    expect(parseMedousaVaultHref("medousa://vault/../x.md")).toBe(null);
  });
});
