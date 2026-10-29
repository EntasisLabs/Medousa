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

it("paints overlapping pages and the live buffer once, keeping the live version", () => {
  const window = new TranscriptWindow("24");
  window.prepend({ cursor: "24", olderCursor: null, messages: messages(0, 25) });
  const live = messages(20, 30);
  live[0] = { ...live[0], content: "Updated live reply", streaming: true };
  const painted = window.messages(live);
  expect(painted.map(message => message.id)).toEqual(messages(0, 30).map(message => message.id));
  expect(painted[20]).toBe(live[0]);
});

it("reconciles a persisted reply with its differently named stream bubble by turn identity", () => {
  const window = new TranscriptWindow("24");
  window.prepend({ cursor: "24", olderCursor: null, messages: [
    { id: "saved", role: "assistant", turnId: "turn", content: "Saved reply" },
  ] });
  const live: ChatMessage = { id: "live", role: "assistant", turnId: "turn", content: "Live reply", streaming: true };
  expect(window.messages([live])).toEqual([live]);
});

it("keeps identical text from distinct turns and different lanes", () => {
  const window = new TranscriptWindow("24");
  const history: ChatMessage[] = [
    { id: "one", role: "assistant", turnId: "one", content: "Done" },
    { id: "two", role: "assistant", turnId: "two", content: "Done" },
    { id: "worker", role: "assistant", turnId: "two", content: "Done", lane: "worker" },
  ];
  window.prepend({ cursor: "24", olderCursor: null, messages: history });
  expect(window.messages([])).toEqual(history);
});

it("bounds live growth while preserving active messages outside the recent tail", () => {
  const all = messages(0, 500);
  all[0].streaming = true;
  const kept = boundedLiveTranscript(all, new Set(["1"]));
  expect(kept).toHaveLength(50);
  expect(kept.slice(0, 2).map(message => message.id)).toEqual(["0", "1"]);
  expect(kept.at(-1)?.id).toBe("499");
});
