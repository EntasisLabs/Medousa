import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FeedEvent } from "$lib/types/environment";

vi.mock("$lib/daemon", () => ({
  fetchFeedTail: vi.fn(),
}));

import { fetchFeedTail } from "$lib/daemon";
import {
  formatFeedEmittedHint,
  mapFeedEventToTile,
  readFeedTail,
  resolveFeedField,
  subscribeFeedTail,
} from "./feedTail";

const fetchMock = vi.mocked(fetchFeedTail);

function event(partial: Partial<FeedEvent> & Pick<FeedEvent, "summary">): FeedEvent {
  return {
    id: partial.id ?? "e1",
    feedId: partial.feedId ?? "trip.pulse",
    emittedAtUtc: partial.emittedAtUtc ?? "2026-07-10T12:00:00.000Z",
    source: partial.source ?? "agent",
    summary: partial.summary,
    refs: partial.refs,
    payload: partial.payload,
  };
}

describe("resolveFeedField", () => {
  it("defaults to summary", () => {
    expect(resolveFeedField(event({ summary: "On time" }))).toBe("On time");
    expect(resolveFeedField(event({ summary: "On time" }), "summary")).toBe("On time");
  });

  it("reads payload.value and nested paths", () => {
    const ev = event({
      summary: "ignored",
      payload: { value: "42%", nested: { score: 9 } },
    });
    expect(resolveFeedField(ev, "payload.value")).toBe("42%");
    expect(resolveFeedField(ev, "payload.nested.score")).toBe("9");
  });

  it("reads source and emittedAtUtc", () => {
    const ev = event({
      summary: "x",
      source: "recurring_job",
      emittedAtUtc: "2026-01-01T00:00:00.000Z",
    });
    expect(resolveFeedField(ev, "source")).toBe("recurring_job");
    expect(resolveFeedField(ev, "emittedAtUtc")).toBe("2026-01-01T00:00:00.000Z");
  });

  it("returns null for missing paths", () => {
    expect(resolveFeedField(event({ summary: "x" }), "payload.missing")).toBeNull();
  });
});

describe("mapFeedEventToTile", () => {
  it("maps value/delta/tone and relative hint when keepHint is false", () => {
    const ev = event({
      summary: "Trains ok",
      emittedAtUtc: new Date(Date.now() - 120_000).toISOString(),
      payload: { delta: "+1", tone: "success" },
    });
    const mapped = mapFeedEventToTile(ev);
    expect(mapped.value).toBe("Trains ok");
    expect(mapped.delta).toBe("+1");
    expect(mapped.tone).toBe("success");
    expect(mapped.hint).toBe("2m ago");
  });

  it("keeps markdown hint when keepHint is true", () => {
    const mapped = mapFeedEventToTile(event({ summary: "x" }), undefined, { keepHint: true });
    expect(mapped.hint).toBeUndefined();
  });
});

describe("formatFeedEmittedHint", () => {
  it("formats relative whispers", () => {
    const now = Date.parse("2026-07-10T12:00:00.000Z");
    expect(formatFeedEmittedHint("2026-07-10T11:59:30.000Z", now)).toBe("just now");
    expect(formatFeedEmittedHint("2026-07-10T11:00:00.000Z", now)).toBe("1h ago");
  });
});

describe("readFeedTail / subscribeFeedTail", () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns the latest event from the tail", async () => {
    fetchMock.mockResolvedValue({
      feedId: "trip.pulse",
      events: [
        event({ id: "e1", summary: "old" }),
        event({ id: "e2", summary: "new" }),
      ],
    });
    await expect(readFeedTail("trip.pulse")).resolves.toMatchObject({ summary: "new" });
  });

  it("returns null on empty tail or fetch error", async () => {
    fetchMock.mockResolvedValue({ feedId: "trip.pulse", events: [] });
    await expect(readFeedTail("trip.pulse")).resolves.toBeNull();

    fetchMock.mockRejectedValue(new Error("offline"));
    await expect(readFeedTail("trip.pulse")).resolves.toBeNull();
  });

  it("polls immediately and on interval until unsubscribed", async () => {
    fetchMock.mockResolvedValue({
      feedId: "trip.pulse",
      events: [event({ summary: "pulse" })],
    });
    const seen: Array<string | null> = [];
    const off = subscribeFeedTail(
      "trip.pulse",
      (ev) => {
        seen.push(ev?.summary ?? null);
      },
      { intervalMs: 15_000 },
    );

    await vi.advanceTimersByTimeAsync(0);
    expect(seen).toEqual(["pulse"]);

    await vi.advanceTimersByTimeAsync(15_000);
    expect(seen).toEqual(["pulse", "pulse"]);

    off();
    await vi.advanceTimersByTimeAsync(15_000);
    expect(seen).toEqual(["pulse", "pulse"]);
  });
});
