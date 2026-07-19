import { describe, expect, it } from "vitest";
import {
  WIZARD_PACKAGE_OFFER_FALLBACK,
  WIZARD_PACKAGE_OFFER_IDS,
} from "./wizardPackageOffers";

describe("wizardPackageOffers", () => {
  it("offers channel adapters and MCP gateway only", () => {
    expect([...WIZARD_PACKAGE_OFFER_IDS]).toEqual([
      "adapter-discord",
      "adapter-telegram",
      "adapter-whatsapp",
      "mcp-gateway",
    ]);
    expect(WIZARD_PACKAGE_OFFER_FALLBACK.map((row) => row.id)).toEqual([
      ...WIZARD_PACKAGE_OFFER_IDS,
    ]);
    expect(WIZARD_PACKAGE_OFFER_IDS).not.toContain("cli");
    expect(WIZARD_PACKAGE_OFFER_IDS).not.toContain("local-brain");
    expect(WIZARD_PACKAGE_OFFER_IDS).not.toContain("adapter-slack");
  });
});
