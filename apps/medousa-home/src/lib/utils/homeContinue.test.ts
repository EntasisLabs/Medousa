import { describe, expect, it } from "vitest";
import type { SessionSummary } from "$lib/types/session";
import type { RepositoryCatalogEntry } from "$lib/forge";
import {
  homeActivityWhisper,
  homeContinueRows,
  homeNotesDateParts,
  homeProjectRows,
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

  it("omits cancelled and throwaway conversations", () => {
    const rows = homeContinueRows([
      session("a", "work-cancel-test (Cancelled)", "Cancelled"),
      session("b", "hi", "hi"),
      session("c", "Heeeey", "Heeeey"),
      session("d", "Plan mom's birthday", "Make the reservation"),
    ]);

    expect(rows.map((row) => row.title)).toEqual(["Plan mom's birthday"]);
  });

  it("deduplicates repeated titles and drops redundant previews", () => {
    const rows = homeContinueRows([
      session("new", "Garden plans", "Garden plans"),
      session("old", "garden-plans", "Earlier version"),
      session("other", "Dinner", "Book a table"),
    ]);

    expect(rows.map((row) => row.sessionId)).toEqual(["new", "other"]);
    expect(rows[0].preview).toBe("");
  });
});

describe("homeProjectRows", () => {
  function repo(
    overrides: Partial<RepositoryCatalogEntry> &
      Pick<RepositoryCatalogEntry, "path" | "display_name" | "last_used_at">,
  ): RepositoryCatalogEntry {
    return {
      repo_id: overrides.path,
      pinned: false,
      archived: false,
      available: true,
      dirty: false,
      changed_files: 0,
      remotes: [],
      existing_projects: [],
      state_explanation: "",
      trust_explanation: "",
      current_branch: "main",
      suggested_base_ref: "main",
      ...overrides,
    };
  }

  it("orders pinned then most recently used", () => {
    const rows = homeProjectRows([
      repo({
        path: "/a",
        display_name: "alpha",
        last_used_at: new Date(Date.now() - 60_000).toISOString(),
      }),
      repo({
        path: "/b",
        display_name: "bravo",
        last_used_at: new Date(Date.now() - 3_600_000).toISOString(),
        pinned: true,
      }),
      repo({
        path: "/c",
        display_name: "charlie",
        last_used_at: new Date(Date.now() - 5_000).toISOString(),
      }),
    ]);
    expect(rows.map((row) => row.title)).toEqual(["bravo", "charlie", "alpha"]);
  });

  it("skips unavailable and archived repos", () => {
    const rows = homeProjectRows([
      repo({
        path: "/gone",
        display_name: "gone",
        last_used_at: new Date().toISOString(),
        available: false,
      }),
      repo({
        path: "/old",
        display_name: "old",
        last_used_at: new Date().toISOString(),
        archived: true,
      }),
      repo({
        path: "/ok",
        display_name: "ok",
        last_used_at: new Date().toISOString(),
      }),
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0].title).toBe("ok");
  });

  it("prefers an active existing project for open target", () => {
    const rows = homeProjectRows([
      repo({
        path: "/medousa",
        display_name: "Medousa",
        last_used_at: new Date().toISOString(),
        existing_projects: [
          {
            id: "done-1",
            title: "Finished polish",
            state: "done",
            human_phase: "done",
          },
          {
            id: "work-1",
            title: "Home projects",
            state: "active",
            human_phase: "work",
          },
        ],
      }),
    ]);
    expect(rows[0].workId).toBe("work-1");
    expect(rows[0].preview).toBe("Home projects");
  });

  it("keeps branch names off Home", () => {
    const rows = homeProjectRows([
      repo({
        path: "/medousa",
        display_name: "Medousa",
        last_used_at: new Date().toISOString(),
        current_branch: "feat/calendar-agenda",
      }),
    ]);

    expect(rows[0].preview).toBe("Open project");
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
