import { describe, expect, it } from "vitest";
import {
  defaultWorkshopRegistry,
  type WorkshopRegistry,
} from "$lib/types/workshopRegistry";
import { secretWorkshopScope } from "$lib/utils/secretWorkshopScope";

function withRemoteActive(): WorkshopRegistry {
  const registry = defaultWorkshopRegistry(new Date("2026-08-24T00:00:00Z"));
  registry.workshops.push({
    id: "remote",
    label: "Remote",
    kind: "portal",
    url: "https://workshop.example",
    createdAt: "2026-08-24T00:00:00.000Z",
    updatedAt: "2026-08-24T00:00:00.000Z",
    pairing: {
      pairingId: "pairing",
      phoneId: "phone",
      workshopDeviceId: "daemon",
      pairedAt: "2026-08-24T00:00:00.000Z",
    },
  });
  registry.activeWorkshopId = "remote";
  return registry;
}

describe("secretWorkshopScope", () => {
  it("uses the embedded credential port only for iOS Personal", () => {
    expect(secretWorkshopScope(defaultWorkshopRegistry(), true)).toBe("embedded");
    expect(secretWorkshopScope(defaultWorkshopRegistry(), false)).toBe(
      "local-transport",
    );
  });

  it("never grants a remote workshop local secret fallback", () => {
    expect(secretWorkshopScope(withRemoteActive(), true)).toBe("remote");
    expect(secretWorkshopScope(withRemoteActive(), false)).toBe("remote");
  });
});
