/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { liveNodeViewStopEvent } from "./liveNodeViewStopEvent";

function targetEvent(type: string, target: Element): Event {
  const event = new Event(type, { bubbles: true });
  Object.defineProperty(event, "target", { value: target });
  return event;
}

describe("liveNodeViewStopEvent", () => {
  it("lets click bubble for Configure / wikilink routing", () => {
    const btn = document.createElement("button");
    expect(liveNodeViewStopEvent(targetEvent("click", btn))).toBe(false);
  });

  it("stops mousedown inside organism UI so clicks fire", () => {
    const btn = document.createElement("button");
    expect(liveNodeViewStopEvent(targetEvent("mousedown", btn))).toBe(true);
  });

  it("allows mousedown on the fence root for atom selection", () => {
    const host = document.createElement("div");
    host.setAttribute("data-fence-block", "");
    host.className = "vault-live-organism-host";
    expect(liveNodeViewStopEvent(targetEvent("mousedown", host))).toBe(false);
  });

  it("stops keyboard events for nested contenteditables", () => {
    const title = document.createElement("p");
    title.contentEditable = "true";
    expect(liveNodeViewStopEvent(targetEvent("keydown", title))).toBe(true);
  });
});
