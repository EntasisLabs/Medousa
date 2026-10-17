import { expect, it } from "vitest";
import { boundedLiveTranscript, TranscriptWindow } from "./transcriptWindow";
import type { ChatMessage } from "$lib/types/chat";

const messages = (from: number, to: number): ChatMessage[] => Array.from({ length: to - from }, (_, i) => ({ id: `${from + i}`, role: "assistant", content: `entry ${from + i}` }));

it("keeps three nearby pages and only reload coordinates for discarded payloads", () => {
  const window = new TranscriptWindow("120");
  for (let seq = 120; seq > 0; seq -= 24) window.prepend({ cursor: String(seq), olderCursor: seq > 24 ? String(seq - 24) : null, messages: messages(seq - 24, seq) });
  expect(window.pages).toHaveLength(3);
  expect(window.messages(messages(120, 144)).map(message => message.id)).toEqual(messages(0, 72).map(message => message.id));
  expect(window.newer).toEqual([null, "120", "96"]);
  expect(window.olderCursor).toBeNull();
  window.append({ cursor: "96", olderCursor: "72", messages: messages(72, 96) });
  expect(window.pages).toHaveLength(3);
  expect(window.messages([]).map(message => message.id)).toEqual(messages(24, 96).map(message => message.id));
  expect(window.olderCursor).toBe("24");
});

it("reads live messages from the stream buffer rather than keeping a stale latest-page copy", () => {
  const window = new TranscriptWindow("24");
  window.prepend({ cursor: "24", olderCursor: null, messages: messages(0, 24) });
  const live = messages(24, 49);
  live.at(-1)!.streaming = true;
  expect(window.messages(live).at(-1)).toBe(live.at(-1));
  expect(window.atLatest).toBe(true);
  expect(window.pages.at(-1)!.messages).toEqual([]);
});

it("bounds live growth while preserving active messages outside the recent tail", () => {
  const all = messages(0, 500);
  all[0].streaming = true;
  const kept = boundedLiveTranscript(all, new Set(["1"]));
  expect(kept).toHaveLength(50);
  expect(kept.slice(0, 2).map(message => message.id)).toEqual(["0", "1"]);
  expect(kept.at(-1)?.id).toBe("499");
});
