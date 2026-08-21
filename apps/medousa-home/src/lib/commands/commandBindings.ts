/**
 * Thin remappable chord map for a small allowlist of workbench commands.
 * Full keybinding editor / profiles / when-clauses stay HCP-11B.
 */

const STORAGE_KEY = "medousa-home-command-bindings-v1";

/** In-memory fallback when localStorage is unavailable (tests / SSR). */
let memoryOverrides: Record<string, string> = {};

/** Stable command id → default chord token (see keyboardShortcutsCatalog). */
export const DEFAULT_COMMAND_CHORDS: Record<string, string> = {
  "workbench.action.showCommands": "mod:Shift+P",
  "workbench.action.quickOpen": "mod:P",
  "workbench.action.navigateBack": "literal:Alt+←",
  "workbench.action.navigateForward": "literal:Alt+→",
  "workbench.actions.view.problems": "literal:—",
  "workbench.action.terminal.toggleTerminal": "mod:`",
  "workbench.view.testing": "literal:—",
  "workbench.action.findInFiles": "mod:Shift+F",
  "workbench.action.files.saveAll": "mod:Shift+S",
  "editor.action.formatDocument": "literal:Shift+Alt+F",
  "editor.action.rename": "literal:F2",
};

export type RemappableCommandId = keyof typeof DEFAULT_COMMAND_CHORDS;

function readOverrides(): Record<string, string> {
  if (typeof localStorage === "undefined") {
    return { ...memoryOverrides };
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...memoryOverrides };
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const out: Record<string, string> = { ...memoryOverrides };
    for (const [id, value] of Object.entries(parsed)) {
      if (typeof value === "string" && value.trim() && id in DEFAULT_COMMAND_CHORDS) {
        out[id] = value.trim();
      }
    }
    return out;
  } catch {
    return { ...memoryOverrides };
  }
}

function writeOverrides(next: Record<string, string>) {
  memoryOverrides = { ...next };
  if (typeof localStorage === "undefined") return;
  if (Object.keys(next).length === 0) {
    localStorage.removeItem(STORAGE_KEY);
    return;
  }
  localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
}

export function defaultChordFor(commandId: string): string | null {
  return DEFAULT_COMMAND_CHORDS[commandId] ?? null;
}

/** Effective chord after local overrides (allowlisted ids only). */
export function effectiveChordFor(commandId: string): string | null {
  const defaults = DEFAULT_COMMAND_CHORDS[commandId];
  if (!defaults) return null;
  const overrides = readOverrides();
  return overrides[commandId] ?? defaults;
}

export function setChordOverride(commandId: string, chord: string | null) {
  if (!(commandId in DEFAULT_COMMAND_CHORDS)) return;
  const overrides = readOverrides();
  if (!chord || chord.trim() === DEFAULT_COMMAND_CHORDS[commandId]) {
    delete overrides[commandId];
  } else {
    overrides[commandId] = chord.trim();
  }
  writeOverrides(overrides);
}

export function clearChordOverrides() {
  memoryOverrides = {};
  writeOverrides({});
}

function normalizedEventKey(event: KeyboardEvent): string {
  if (event.key === "ArrowLeft") return "←";
  if (event.key === "ArrowRight") return "→";
  if (event.key === "ArrowUp") return "↑";
  if (event.key === "ArrowDown") return "↓";
  if (event.key === " ") return "Space";
  return event.key.length === 1 ? event.key.toUpperCase() : event.key;
}

/** Convert a keyboard event into the persisted, platform-neutral chord grammar. */
export function chordFromKeyboardEvent(event: KeyboardEvent): string | null {
  if (["Meta", "Control", "Shift", "Alt"].includes(event.key)) return null;
  const key = normalizedEventKey(event);
  const modifiers: string[] = [];
  if (event.shiftKey) modifiers.push("Shift");
  if (event.altKey) modifiers.push("Alt");
  const usesMod = event.metaKey || event.ctrlKey;
  if (!usesMod && modifiers.length === 0 && !/^F\d{1,2}$/.test(key)) return null;
  const body = [...modifiers, key].join("+");
  return `${usesMod ? "mod" : "literal"}:${body}`;
}

export function eventMatchesCommandChord(
  event: KeyboardEvent,
  commandId: string,
): boolean {
  const expected = effectiveChordFor(commandId);
  if (!expected || expected === "literal:—") return false;
  return chordFromKeyboardEvent(event) === expected;
}

export function conflictingCommandForChord(
  commandId: string,
  chord: string,
): string | null {
  for (const id of Object.keys(DEFAULT_COMMAND_CHORDS)) {
    if (id !== commandId && effectiveChordFor(id) === chord) return id;
  }
  return null;
}

export function listRemappableBindings(): Array<{
  commandId: string;
  defaultChord: string;
  effectiveChord: string;
  overridden: boolean;
}> {
  return Object.entries(DEFAULT_COMMAND_CHORDS).map(([commandId, defaultChord]) => {
    const effectiveChord = effectiveChordFor(commandId) ?? defaultChord;
    return {
      commandId,
      defaultChord,
      effectiveChord,
      overridden: effectiveChord !== defaultChord,
    };
  });
}
