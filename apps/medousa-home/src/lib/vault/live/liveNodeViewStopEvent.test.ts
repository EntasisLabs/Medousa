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

  it("stops mousedown on fence body so text can be selected", () => {
    const pre = document.createElement("pre");
    pre.className = "vault-live-plain-fence__body";
    expect(liveNodeViewStopEvent(targetEvent("mousedown", pre))).toBe(true);
  });

  it("keeps mouseup with the browser when a fence text selection is active", () => {
    const host = document.createElement("div");
    host.setAttribute("data-fence-block", "");
    host.className = "vault-live-organism-host";
    const pre = document.createElement("pre");
    pre.className = "vault-live-plain-fence__body";
    pre.textContent = "const x = 1";
    host.append(pre);
    document.body.append(host);

    const range = document.createRange();
    range.selectNodeContents(pre);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);

    expect(liveNodeViewStopEvent(targetEvent("mouseup", host))).toBe(true);

    sel?.removeAllRanges();
    host.remove();
  });

  it("stops keyboard events for nested contenteditables", () => {
    const title = document.createElement("p");
    title.contentEditable = "true";
    expect(liveNodeViewStopEvent(targetEvent("keydown", title))).toBe(true);
  });
});
