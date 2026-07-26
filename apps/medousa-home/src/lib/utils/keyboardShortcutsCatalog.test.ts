import { describe, expect, it } from "vitest";
import {
  CHEAT_SHEET_GROUP_IDS,
  KEYBOARD_SHORTCUTS_CATALOG,
  catalogGroup,
  formatCatalogKeys,
} from "./keyboardShortcutsCatalog";

describe("keyboardShortcutsCatalog", () => {
  it("lists expected groups with stable entry ids", () => {
    expect(KEYBOARD_SHORTCUTS_CATALOG.map((g) => g.id)).toEqual([
      "global",
      "panes",
      "vault",
      "chat",
    ]);
    const panes = catalogGroup("panes");
    expect(panes?.entries.map((e) => e.id)).toContain("split-right");
    expect(panes?.entries.map((e) => e.id)).toContain("zoom-pane");
    expect(CHEAT_SHEET_GROUP_IDS).toEqual(["global", "panes", "vault"]);
  });

  it("formats prefix and mod chords", () => {
    const prefix = formatCatalogKeys("prefix:?");
    expect(prefix === "Ctrl+; ?" || prefix.endsWith("; ?")).toBe(true);

    const spotlight = formatCatalogKeys("mod:K");
    expect(spotlight === "Ctrl+K" || spotlight === "⌘K").toBe(true);

    expect(formatCatalogKeys("literal:Drag tab")).toBe("Drag tab");
  });

  it("keeps catalog actions aligned with real product binds", () => {
    const snapshot = KEYBOARD_SHORTCUTS_CATALOG.flatMap((group) =>
      group.entries.map((entry) => `${group.id}:${entry.id}:${entry.keys}:${entry.action}`),
    );
    expect(snapshot).toEqual([
      "global:spotlight:mod:K:Open Spotlight",
      "global:toggle-rail:mod:B:Toggle left rail",
      "global:summon-toolbar:mod:Shift+.:Summon view toolbar",
      "global:content-zoom:mod:+ / −:Zoom in / out",
      "global:content-zoom-reset:mod:0:Reset zoom",
      "global:keyboard-shortcuts:prefix:?:Open keyboard shortcuts",
      "panes:split-right:prefix:%:Split right",
      'panes:split-down:prefix:":Split down',
      "panes:focus-panes:prefix:h/j/k/l:Focus pane",
      "panes:zoom-pane:prefix:z:Zoom pane",
      "panes:close-pane:prefix:x:Close pane (merge tabs)",
      "panes:chat-tab:prefix:c:Chat tab here",
      "panes:next-prev-tab:prefix:n/p:Next / prev tab",
      "panes:show-tabs:prefix:w:Show tabs",
      "panes:switch-desktop:prefix:1–4:Switch virtual desktop",
      "panes:drag-tab:literal:Drag tab:Move tab to another pane",
      "vault:vault-save:mod:S:Save note",
      "vault:vault-find:mod:F:Find in note",
      "vault:vault-new:mod:N:New note",
      "vault:vault-plane:mod:Shift+E:Toggle edit / preview plane",
      "vault:vault-pdf:mod:Shift+P:Export PDF",
      "vault:vault-board:mod:Shift+B:Toggle board",
      "chat:chat-spotlight:mod:K:Spotlight (commands & jumps)",
      "chat:chat-open-shortcuts:prefix:?:Keyboard shortcuts sheet",
    ]);
  });
});
