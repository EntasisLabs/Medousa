import { describe, expect, it } from "vitest";
import { permittedCarPlayAction } from "./carPlayLive";

describe("CarPlay Live action ownership", () => {
  it("starts only when idle and applies controls only to active Live", () => {
    expect(permittedCarPlayAction({ owner: "chat", action: "start" }, "chat", false, false)?.action).toBe("start");
    expect(permittedCarPlayAction({ owner: "chat", action: "start" }, "chat", true, false)).toBeNull();
    for (const action of ["mute", "unmute", "stop"]) {
      expect(permittedCarPlayAction({ owner: "chat", action }, "chat", true, false)?.action).toBe(action);
      expect(permittedCarPlayAction({ owner: "chat", action }, "chat", false, false)).toBeNull();
    }
  });
  it("rejects stale owners, busy controls, malformed actions and arbitrary commands", () => {
    for (const value of [null, "stop", {}, { owner: "old", action: "stop" }, { owner: "chat", action: "execute" }]) {
      expect(permittedCarPlayAction(value, "chat", true, false)).toBeNull();
    }
    expect(permittedCarPlayAction({ owner: "chat", action: "stop" }, "chat", true, true)).toBeNull();
  });
});
