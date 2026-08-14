import { describe, expect, it } from "vitest";
import { capabilityBadges } from "$lib/utils/lanShareApi";

describe("lanShareApi", () => {
  it("parses capability bitfield badges", () => {
    expect(capabilityBadges("003F")).toEqual(["Share", "Layouts", "Relay"]);
    expect(capabilityBadges(null)).toEqual([]);
  });

  it("treats missing flags as empty badges", () => {
    expect(capabilityBadges(undefined)).toEqual([]);
    expect(capabilityBadges("not-hex")).toEqual([]);
  });

  it("maps share and layout bits independently", () => {
    expect(capabilityBadges("0008")).toEqual(["Share"]);
    expect(capabilityBadges("0010")).toEqual(["Layouts"]);
    expect(capabilityBadges("0018")).toEqual(["Share", "Layouts"]);
  });

  it("documents invite-based workshop trust as Tauri-only", async () => {
    const { trustWorkshopFromQr } = await import("$lib/utils/lanShareApi");
    await expect(
      trustWorkshopFromQr({
        qrUrl: "medousa://pair/1.0?a=192.168.1.2%3A7419&d=device&t=token&s=signature&u=key",
        daemonUrl: "http://192.168.1.2:7419",
        workshopName: "Studio",
      }),
    ).rejects.toThrow(/desktop app/i);
  });
});
