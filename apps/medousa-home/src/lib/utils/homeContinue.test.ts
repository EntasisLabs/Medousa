import { describe, expect, it } from "vitest";
import type { SessionSummary } from "$lib/types/session";
import {
  homeActivityWhisper,
  homeContinueRows,
  homeNotesDateParts,
  peerInitials,
  relativeSessionTime,
  stripMarkdownPreview,
} from "$lib/utils/homeContinue";

function session(
  id: string,
  name: string,
  preview: string,
  last?: string,
): SessionSummary {
  return {
    session_id: id,
    display_name: name,
    turns: 1,
    verification_runs: 0,
    last_timestamp: last ?? null,
    preview,
  };
}

describe("stripMarkdownPreview", () => {
  it("strips bold markers from previews", () => {
    expect(
      stripMarkdownPreview("Claude Opus 5 just landed **yesterday** — great timing."),
    ).toBe("Claude Opus 5 just landed yesterday — great timing.");
  });

  it("strips links and inline code", () => {
    expect(stripMarkdownPreview("See [docs](https://x.test) and `code`")).toBe(
      "See docs and code",
    );
  });
});

describe("homeContinueRows", () => {
  it("returns lead plus up to two whispers", () => {
    const rows = homeContinueRows([
      session("a", "Vault sync", "First line\nSecond"),
      session("b", "Weekly", "Next"),
      session("c", "Dinner", "Plan"),
      session("d", "Extra", "Skip"),
    ]);
    expect(rows).toHaveLength(3);
    expect(rows[0].sessionId).toBe("a");
    expect(rows[0].title).toBe("Vault sync");
    expect(rows[0].preview).toBe("First line");
    expect(rows[2].sessionId).toBe("c");
  });

  it("strips markdown in previews", () => {
    const rows = homeContinueRows([
      session("a", "Research", "Landed **yesterday** on Opus"),
    ]);
    expect(rows[0].preview).toBe("Landed yesterday on Opus");
  });

  it("returns empty when there are no sessions", () => {
    expect(homeContinueRows([])).toEqual([]);
  });
});

describe("relativeSessionTime", () => {
  it("formats recent minutes", () => {
    const iso = new Date(Date.now() - 5 * 60_000).toISOString();
    expect(relativeSessionTime(iso)).toBe("5m");
  });
});

describe("homeNotesDateParts", () => {
  it("returns weekday and day number", () => {
    const parts = homeNotesDateParts(new Date("2026-07-26T12:00:00"));
    expect(parts.weekday).toBe("Sunday");
    expect(parts.day).toBe("26");
  });
});

describe("peerInitials", () => {
  it("builds two-letter initials", () => {
    expect(peerInitials("Alex Morgan")).toBe("AM");
    expect(peerInitials("Sam")).toBe("SA");
  });
});

describe("homeActivityWhisper", () => {
  it("drops redundant Needs you · Stuck lines", () => {
    expect(
      homeActivityWhisper("Needs you", "Stuck — needs a look", "Needs you · Stuck"),
    ).toBeNull();
  });

  it("keeps distinct whispers", () => {
    expect(
      homeActivityWhisper("Needs you", "Vault sync", "Waiting on your approval"),
    ).toBe("Waiting on your approval");
  });
});
