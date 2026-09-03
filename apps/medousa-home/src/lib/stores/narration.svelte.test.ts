/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NarrationStore } from "$lib/stores/narration.svelte";

class FakeUtterance {
  lang = "";
  rate = 1;
  onend: (() => void) | null = null;
  onerror: ((event: { error: string }) => void) | null = null;

  constructor(readonly text: string) {}
}

describe("NarrationStore", () => {
  let spoken: FakeUtterance[];
  let cancel: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    localStorage.clear();
    spoken = [];
    cancel = vi.fn();
    vi.stubGlobal("SpeechSynthesisUtterance", FakeUtterance);
    Object.defineProperty(window, "speechSynthesis", {
      configurable: true,
      value: {
        cancel,
        speak: (utterance: FakeUtterance) => spoken.push(utterance),
      },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it("persists automatic narration and speaks only finalized prose once per turn", () => {
    const store = new NarrationStore();
    store.initialize();
    store.setAutoNarrate(true);

    store.maybeAutoNarrate("turn-1", "message-1", "**Grounded answer.**");
    store.maybeAutoNarrate("turn-1", "message-1", "**Grounded answer.**");

    expect(store.available).toBe(true);
    expect(store.autoNarrate).toBe(true);
    expect(localStorage.getItem("medousa-home-auto-narrate")).toBe("1");
    expect(spoken.map((utterance) => utterance.text)).toEqual(["Grounded answer."]);
    expect(store.activeMessageId).toBe("message-1");
  });

  it("plays bounded chunks in order and clears the active message when finished", () => {
    const store = new NarrationStore();
    const longReply = `${"First concept. ".repeat(20)}Second concept.`;

    expect(store.speak("message-2", longReply)).toBe(true);
    expect(spoken).toHaveLength(1);
    spoken[0].onend?.();
    expect(spoken.length).toBeGreaterThan(1);
    expect(spoken.every((utterance) => utterance.text.length <= 280)).toBe(true);

    while (store.activeMessageId && spoken.at(-1)?.onend) {
      const current = spoken.at(-1);
      current?.onend?.();
      current!.onend = null;
    }
    expect(store.activeMessageId).toBeNull();
  });

  it("cancels speech when narration is turned off", () => {
    const store = new NarrationStore();
    store.setAutoNarrate(true);
    store.speak("message-3", "A response.");

    store.setAutoNarrate(false);

    expect(store.autoNarrate).toBe(false);
    expect(store.activeMessageId).toBeNull();
    expect(cancel).toHaveBeenCalled();
  });
});
