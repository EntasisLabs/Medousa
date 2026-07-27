import { describe, expect, it } from "vitest";
import { filterTabSearchHits, hitMatchesQuery, type ShellTabSearchHit } from "./shellTabSearch";

function hit(partial: Partial<ShellTabSearchHit> & Pick<ShellTabSearchHit, "tabId" | "title">): ShellTabSearchHit {
  return {
    kind: "chat",
    desktopId: "d1",
    desktopName: "Main",
    groupId: "g1",
    paneIndex: 1,
    isActive: false,
    isActiveDesktop: true,
    ...partial,
  };
}

describe("shellTabSearch", () => {
  it("matches title, kind, desktop, and pane", () => {
    const row = hit({ tabId: "t1", title: "Launch notes", kind: "lme", desktopName: "Research", paneIndex: 2 });
    expect(hitMatchesQuery(row, "launch")).toBe(true);
    expect(hitMatchesQuery(row, "LME")).toBe(true);
    expect(hitMatchesQuery(row, "research")).toBe(true);
    expect(hitMatchesQuery(row, "pane 2")).toBe(true);
    expect(hitMatchesQuery(row, "missing")).toBe(false);
  });

  it("pins the active tab when the query is empty", () => {
    const hits = [
      hit({ tabId: "a", title: "A", isActive: false, isActiveDesktop: true }),
      hit({ tabId: "b", title: "B", isActive: true, isActiveDesktop: true }),
      hit({
        tabId: "c",
        title: "C",
        desktopId: "d2",
        desktopName: "Two",
        isActiveDesktop: false,
      }),
    ];
    expect(filterTabSearchHits(hits, "").map((row) => row.tabId)).toEqual(["b", "a", "c"]);
  });

  it("filters by query without reordering beyond matches", () => {
    const hits = [
      hit({ tabId: "a", title: "Alpha chat" }),
      hit({ tabId: "b", title: "Beta doc", kind: "lme" }),
    ];
    expect(filterTabSearchHits(hits, "beta").map((row) => row.tabId)).toEqual(["b"]);
  });
});
