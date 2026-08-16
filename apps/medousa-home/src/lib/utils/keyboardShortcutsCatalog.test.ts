import { describe, expect, it } from "vitest";
import {
  CHEAT_SHEET_GROUP_IDS,
  KEYBOARD_SHORTCUTS_CATALOG,
  catalogGroup,
  formatCatalogKeys,
  shortcutEntryById,
  titleWithKeys,
  titleWithShortcut,
} from "./keyboardShortcutsCatalog";

describe("keyboardShortcutsCatalog", () => {
  it("lists expected groups with stable entry ids", () => {
    expect(KEYBOARD_SHORTCUTS_CATALOG.map((g) => g.id)).toEqual([
      "global",
      "panes",
      "code",
      "review",
      "vault",
      "browser",
      "chat",
    ]);
    const panes = catalogGroup("panes");
    expect(panes?.entries.map((e) => e.id)).toContain("split-right");
    expect(panes?.entries.map((e) => e.id)).toContain("zoom-pane");
    expect(CHEAT_SHEET_GROUP_IDS).toEqual([
      "global",
      "panes",
      "code",
      "vault",
      "browser",
      "chat",
    ]);
  });

  it("formats prefix chords as two-step sequences", () => {
    const prefix = formatCatalogKeys("prefix:?");
    expect(prefix === "Ctrl+; + ?" || prefix.endsWith("; + ?")).toBe(true);

    const spotlight = formatCatalogKeys("mod:K");
    expect(spotlight === "Ctrl+K" || spotlight === "⌘K").toBe(true);

    expect(formatCatalogKeys("literal:Drag tab")).toBe("Drag tab");
  });

  it("looks up entries and builds button titles", () => {
    expect(shortcutEntryById("toggle-rail")?.keys).toBe("mod:B");
    expect(shortcutEntryById("missing")).toBeUndefined();

    const rail = titleWithShortcut("Expand navigation rail", "toggle-rail");
    expect(rail === "Expand navigation rail (⌘B)" || rail === "Expand navigation rail (Ctrl+B)").toBe(
      true,
    );
    expect(titleWithShortcut("Nope", "does-not-exist")).toBe("Nope");

    const replace = titleWithKeys("Replace", "mod:⌥F");
    expect(replace === "Replace (⌘⌥F)" || replace === "Replace (Ctrl+Alt+F)").toBe(true);
  });

  it("keeps catalog actions aligned with real product binds", () => {
    const snapshot = KEYBOARD_SHORTCUTS_CATALOG.flatMap((group) =>
      group.entries.map((entry) => `${group.id}:${entry.id}:${entry.keys}:${entry.action}`),
    );
    expect(snapshot).toEqual([
      "global:spotlight:mod:K:Open Spotlight",
      "global:command-palette:mod:Shift+P:Show All Commands (Spotlight >)",
      "global:toggle-rail:mod:B:Toggle left rail",
      "global:summon-toolbar:mod:Shift+.:Summon view toolbar",
      "global:content-zoom:mod:+ / −:Zoom in / out",
      "global:content-zoom-reset:mod:0:Reset zoom",
      "global:open-notes:mod:O:Open notes",
      "global:keyboard-shortcuts:mod:/:Open keyboard shortcuts",
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
      "code:code-quick-open:mod:P:Quick Open file / symbol / line",
      "code:code-search:mod:Shift+F:Search in files",
      "code:code-save:mod:S:Save focused file",
      "code:code-save-all:mod:Shift+S:Save all modified files",
      "code:code-find:mod:F:Find in file",
      "code:code-structure:mod:Shift+O:Structure (symbols)",
      "code:code-reopen:mod:Shift+T:Reopen last closed file",
      "code:code-terminal:mod:`:Toggle Terminal dock",
      "code:code-rename:literal:F2:Rename symbol",
      "code:code-cycle-tabs:literal:Ctrl+Tab:Cycle Code file tabs in focused pane",
      "review:review-next-file:literal:n / j:Next changed file",
      "review:review-prev-file:literal:p / k:Previous changed file",
      "review:review-toggle-viewed:literal:v:Toggle file viewed",
      "review:review-comment:literal:.:Add comment on focused file",
      "review:review-toggle-comments:literal:c:Show or hide the comments rail when threads exist",
      "vault:vault-save:mod:S:Save note",
      "vault:vault-find:mod:F:Find in note",
      "vault:vault-new:mod:N:New note",
      "vault:vault-plane:mod:Shift+E:Toggle edit / preview plane",
      "vault:vault-pdf:mod:Shift+P:Export PDF",
      "vault:vault-board:mod:Shift+B:Toggle board",
      "browser:browser-focus-url:mod:L:Focus URL bar",
      "browser:browser-find:mod:F:Find in page",
      "browser:browser-bookmarks:mod:Shift+B:Open bookmarks",
      "browser:browser-new-tab:mod:T:New tab",
      "browser:browser-reopen-tab:mod:Shift+T:Reopen closed tab",
      "browser:browser-close-tab:mod:W:Close tab",
      "browser:browser-reload:mod:R:Reload page",
      "browser:browser-back:mod:[:Go back",
      "browser:browser-forward:mod:]:Go forward",
      "chat:chat-spotlight:mod:K:Spotlight (commands & jumps)",
      "chat:chat-open-shortcuts:mod:/:Keyboard shortcuts sheet",
    ]);
  });
});
