import { describe, expect, it } from "vitest";
import type { VaultNote } from "$lib/types/vault";
import { MAP_KIND_LEGEND, MAP_KIND_VISUALS } from "$lib/utils/contextMapVisual";
import {
  buildNoteGraphSlice,
  HUB_TAG_NOTE_LIMIT,
  MAX_TAG_EDGES_PER_NOTE,
  noteMapId,
  sessionIdForNoteChatTag,
} from "$lib/utils/contextMapNotes";

function note(partial: Partial<VaultNote> & { path: string }): VaultNote {
  return {
    path: partial.path,
    title: partial.title ?? partial.path,
    byte_size: 0,
    content_hash: "hash",
    modified_at_utc: partial.modified_at_utc ?? "2026-07-20T12:00:00Z",
    created_at_utc: "2026-07-01T00:00:00Z",
    tags: partial.tags ?? [],
    wikilinks_out: partial.wikilinks_out ?? [],
    backlinks: partial.backlinks ?? [],
    kind: partial.kind,
  };
}

describe("sessionIdForNoteChatTag", () => {
  it("matches chat: prefix to a session id", () => {
    const sessionId = "abcdef12-9999-session";
    expect(
      sessionIdForNoteChatTag(["medousa", "chat:abcdef12", "vault"], [sessionId, "other"]),
    ).toBe(sessionId);
  });

  it("returns null when no session matches", () => {
    expect(sessionIdForNoteChatTag(["chat:zzzzzzzz"], ["abcdef12-session"])).toBeNull();
  });
});

describe("buildNoteGraphSlice", () => {
  it("creates note_session edges from chat tags", () => {
    const sessionId = "deadbeef-aaaa";
    const slice = buildNoteGraphSlice(
      [
        note({
          path: "notes/a.md",
          title: "Alpha",
          tags: ["chat:deadbeef", "vault"],
        }),
      ],
      [sessionId],
    );

    expect(slice.nodes.some((node) => node.id === noteMapId("notes/a.md"))).toBe(true);
    expect(
      slice.edges.some(
        (edge) =>
          edge.kind === "note_session" &&
          edge.from === noteMapId("notes/a.md") &&
          edge.to === `session:${sessionId}`,
      ),
    ).toBe(true);
  });

  it("creates note_link for wikilinks and skips workshop tags for note_tag", () => {
    const slice = buildNoteGraphSlice(
      [
        note({
          path: "notes/a.md",
          title: "A",
          tags: ["medousa", "vault", "chat:deadbeef"],
          wikilinks_out: ["notes/b.md"],
        }),
        note({
          path: "notes/b.md",
          title: "B",
          tags: ["vault", "bonsai"],
          backlinks: ["notes/a.md"],
        }),
        note({
          path: "notes/c.md",
          title: "C",
          tags: ["bonsai"],
        }),
      ],
      ["deadbeef-session"],
    );

    expect(slice.edges.some((edge) => edge.kind === "note_link")).toBe(true);
    // Shared human tag connects B↔C (A↔B already has note_link, so no duplicate tag edge).
    expect(
      slice.edges.some(
        (edge) =>
          edge.kind === "note_tag" &&
          edge.id.includes("bonsai") &&
          edge.id.includes(noteMapId("notes/b.md")) &&
          edge.id.includes(noteMapId("notes/c.md")),
      ),
    ).toBe(true);
    expect(
      slice.edges.some(
        (edge) => edge.kind === "note_tag" && edge.id.includes("vault"),
      ),
    ).toBe(false);
  });

  it("skips hub tags and caps tag edges per note", () => {
    const notes = Array.from({ length: HUB_TAG_NOTE_LIMIT + 2 }, (_, index) =>
      note({
        path: `notes/n${index}.md`,
        title: `N${index}`,
        tags: ["hubtag", "rare"],
        modified_at_utc: `2026-07-${String(10 + (index % 20)).padStart(2, "0")}T12:00:00Z`,
      }),
    );
    // Make rare unique pairs among first few only.
    notes[0] = note({
      path: "notes/n0.md",
      title: "N0",
      tags: ["hubtag", "pair"],
    });
    notes[1] = note({
      path: "notes/n1.md",
      title: "N1",
      tags: ["hubtag", "pair"],
    });

    const slice = buildNoteGraphSlice(notes, []);
    expect(
      slice.edges.some((edge) => edge.kind === "note_tag" && edge.id.includes("hubtag")),
    ).toBe(false);

    const pairEdges = slice.edges.filter(
      (edge) => edge.kind === "note_tag" && edge.id.includes("pair"),
    );
    expect(pairEdges.length).toBe(1);

    const counts = new Map<string, number>();
    for (const edge of slice.edges.filter((entry) => entry.kind === "note_tag")) {
      counts.set(edge.from, (counts.get(edge.from) ?? 0) + 1);
      counts.set(edge.to, (counts.get(edge.to) ?? 0) + 1);
    }
    for (const count of counts.values()) {
      expect(count).toBeLessThanOrEqual(MAX_TAG_EDGES_PER_NOTE);
    }
  });
});

describe("map legend", () => {
  it("shows Note live and keeps Memory out of the legend", () => {
    expect(MAP_KIND_VISUALS.note.planned).toBeFalsy();
    expect(MAP_KIND_LEGEND.map((entry) => entry.kind)).toEqual([
      "session",
      "thread",
      "note",
    ]);
    expect(MAP_KIND_LEGEND.some((entry) => entry.kind === "claim")).toBe(false);
  });
});
