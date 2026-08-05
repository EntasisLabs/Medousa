import { formatShortcut, modKeyLabel, usesMetaModKey } from "$lib/platform";

export type ShortcutCatalogGroupId = "global" | "panes" | "code" | "vault" | "chat";

export type ShortcutCatalogEntry = {
  id: string;
  /** Raw chord token for formatting (see `formatCatalogKeys`). */
  keys: string;
  action: string;
};

export type ShortcutCatalogGroup = {
  id: ShortcutCatalogGroupId;
  title: string;
  entries: ShortcutCatalogEntry[];
};

/**
 * Single reference for binds that already exist in the app.
 * Drive cheat sheet / Spotlight labels from here — no remapping UI.
 */
export const KEYBOARD_SHORTCUTS_CATALOG: ShortcutCatalogGroup[] = [
  {
    id: "global",
    title: "Global",
    entries: [
      { id: "spotlight", keys: "mod:K", action: "Open Spotlight" },
      { id: "toggle-rail", keys: "mod:B", action: "Toggle left rail" },
      {
        id: "summon-toolbar",
        keys: "mod:Shift+.",
        action: "Summon view toolbar",
      },
      {
        id: "content-zoom",
        keys: "mod:+ / −",
        action: "Zoom in / out",
      },
      { id: "content-zoom-reset", keys: "mod:0", action: "Reset zoom" },
      { id: "open-notes", keys: "mod:O", action: "Open notes" },
      {
        id: "keyboard-shortcuts",
        keys: "mod:/",
        action: "Open keyboard shortcuts",
      },
    ],
  },
  {
    id: "panes",
    title: "Panes",
    entries: [
      { id: "split-right", keys: "prefix:%", action: "Split right" },
      { id: "split-down", keys: 'prefix:"', action: "Split down" },
      { id: "focus-panes", keys: "prefix:h/j/k/l", action: "Focus pane" },
      { id: "zoom-pane", keys: "prefix:z", action: "Zoom pane" },
      { id: "close-pane", keys: "prefix:x", action: "Close pane (merge tabs)" },
      { id: "chat-tab", keys: "prefix:c", action: "Chat tab here" },
      { id: "next-prev-tab", keys: "prefix:n/p", action: "Next / prev tab" },
      { id: "show-tabs", keys: "prefix:w", action: "Show tabs" },
      {
        id: "switch-desktop",
        keys: "prefix:1–4",
        action: "Switch virtual desktop",
      },
      { id: "drag-tab", keys: "literal:Drag tab", action: "Move tab to another pane" },
    ],
  },
  {
    id: "code",
    title: "Code",
    entries: [
      { id: "code-quick-open", keys: "mod:P", action: "Quick Open file / symbol / line" },
      { id: "code-save", keys: "mod:S", action: "Save focused file" },
      { id: "code-save-all", keys: "mod:Shift+S", action: "Save all modified files" },
      { id: "code-find", keys: "mod:F", action: "Find in file" },
      { id: "code-structure", keys: "mod:Shift+O", action: "Structure (symbols)" },
      { id: "code-reopen", keys: "mod:Shift+T", action: "Reopen last closed file" },
      { id: "code-terminal", keys: "mod:`", action: "Toggle Terminal dock" },
      { id: "code-rename", keys: "literal:F2", action: "Rename symbol" },
      {
        id: "code-cycle-tabs",
        keys: "literal:Ctrl+Tab",
        action: "Cycle Code file tabs in focused pane",
      },
    ],
  },
  {
    id: "vault",
    title: "Vault",
    entries: [
      { id: "vault-save", keys: "mod:S", action: "Save note" },
      { id: "vault-find", keys: "mod:F", action: "Find in note" },
      { id: "vault-new", keys: "mod:N", action: "New note" },
      {
        id: "vault-plane",
        keys: "mod:Shift+E",
        action: "Toggle edit / preview plane",
      },
      { id: "vault-pdf", keys: "mod:Shift+P", action: "Export PDF" },
      { id: "vault-board", keys: "mod:Shift+B", action: "Toggle board" },
    ],
  },
  {
    id: "chat",
    title: "Chat / Spotlight",
    entries: [
      { id: "chat-spotlight", keys: "mod:K", action: "Spotlight (commands & jumps)" },
      {
        id: "chat-open-shortcuts",
        keys: "mod:/",
        action: "Keyboard shortcuts sheet",
      },
    ],
  },
];

/** Format a catalog `keys` token for the current platform. */
export function formatCatalogKeys(keys: string): string {
  if (keys.startsWith("literal:")) {
    return keys.slice("literal:".length);
  }
  if (keys.startsWith("prefix:")) {
    // Two-step chord — spell out the sequence so it doesn't read as one press.
    const suffix = keys.slice("prefix:".length);
    const mod = modKeyLabel();
    return usesMetaModKey() ? `${mod}; then ${suffix}` : `Ctrl+; then ${suffix}`;
  }
  if (keys.startsWith("mod:")) {
    const chord = keys.slice("mod:".length);
    // Multi-key zoom hint — format each side with the mod glyph.
    if (chord.includes(" / ")) {
      return chord
        .split(" / ")
        .map((part) => formatShortcut(part.trim()))
        .join(" / ");
    }
    if (chord.startsWith("Shift+")) {
      const rest = chord.slice("Shift+".length);
      return formatShortcut(`⇧${rest}`);
    }
    return formatShortcut(chord);
  }
  return keys;
}

export function catalogGroup(
  id: ShortcutCatalogGroupId,
): ShortcutCatalogGroup | undefined {
  return KEYBOARD_SHORTCUTS_CATALOG.find((group) => group.id === id);
}

/** Groups shown in the in-app shortcuts sheet. */
export const CHEAT_SHEET_GROUP_IDS: ShortcutCatalogGroupId[] = [
  "global",
  "panes",
  "code",
  "vault",
  "chat",
];
