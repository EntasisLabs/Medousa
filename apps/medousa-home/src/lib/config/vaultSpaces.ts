/** M7a/M7b — space roots for Library tree (folder prefixes, not new storage). */

import {
  customSpacesAsVaultConfig,
  loadCustomVaultSpaces,
} from "$lib/utils/vaultCustomSpaces";

export const SHOW_SYSTEM_NOTES_KEY = "medousa-home-vault-show-system";
export const LAST_SPACE_KEY = "medousa-home-last-space";

export type VaultSpaceIcon =
  | "book"
  | "folder"
  | "wallet"
  | "inbox"
  | "bug"
  | "layers"
  | "settings";

export interface VaultSpaceConfig {
  id: string;
  prefix: string;
  label: string;
  sort: number;
  icon: VaultSpaceIcon;
  alwaysShow?: boolean;
  defaultCollapsed?: boolean;
  /** Shown in space chip row (M7b). */
  filterChip?: boolean;
  /** Hidden until developer notes are enabled (M8f). */
  devOnly?: boolean;
}

export const VAULT_SPACES: VaultSpaceConfig[] = [
  {
    id: "journal",
    prefix: "journal/",
    label: "Journal",
    sort: 0,
    icon: "book",
    alwaysShow: true,
    filterChip: true,
  },
  {
    id: "projects",
    prefix: "projects/",
    label: "Projects",
    sort: 1,
    icon: "folder",
    filterChip: true,
  },
  {
    id: "finance",
    prefix: "finance/",
    label: "Finance",
    sort: 2,
    icon: "wallet",
    filterChip: true,
  },
  {
    id: "inbox",
    prefix: "inbox/",
    label: "Inbox",
    sort: 3,
    icon: "inbox",
    alwaysShow: true,
    filterChip: true,
  },
  {
    id: "bugs",
    prefix: "bugs/",
    label: "Bugs",
    sort: 4,
    icon: "bug",
    filterChip: true,
    devOnly: true,
  },
];

export const VAULT_OTHER_SPACE: VaultSpaceConfig = {
  id: "other",
  prefix: "",
  label: "Other",
  sort: 50,
  icon: "layers",
};

export const VAULT_SYSTEM_BUCKET: VaultSpaceConfig = {
  id: "system_bucket",
  prefix: "",
  label: "System",
  sort: 91,
  icon: "settings",
  defaultCollapsed: true,
};

/** Built-in spaces shown as filter chips above the tree. */
export const VAULT_FILTER_SPACES: VaultSpaceConfig[] = VAULT_SPACES.filter(
  (space) => space.filterChip,
);

/** Built-in + user-created groups for tree and filters. */
export function allVaultSpaces(): VaultSpaceConfig[] {
  return [...VAULT_SPACES, ...customSpacesAsVaultConfig()];
}

export function allFilterSpaces(showDeveloperNotes: boolean): VaultSpaceConfig[] {
  return allVaultSpaces().filter(
    (space) => space.filterChip && (!space.devOnly || showDeveloperNotes),
  );
}

export function isDevSpaceNote(path: string): boolean {
  return path.startsWith("bugs/");
}

/** Notes hidden from the default garage view (system paths + dev spaces). */
export function shouldHideGarageNote(
  path: string,
  title: string,
  showDeveloperNotes: boolean,
): boolean {
  if (showDeveloperNotes) return false;
  return isSystemNoiseNote(path, title) || isDevSpaceNote(path);
}

export function isSystemNoiseNote(path: string, title: string): boolean {
  const lowerPath = path.toLowerCase();
  const lowerTitle = title.toLowerCase();
  if (lowerPath.startsWith(".trash/") || lowerPath.startsWith("system/")) {
    return true;
  }
  if (lowerTitle.startsWith("ask ·") || lowerTitle.startsWith("ask -")) {
    return true;
  }
  if (lowerPath.includes("medousa-daemon")) {
    return true;
  }
  const stem = (path.split("/").pop() ?? path).replace(/\.md$/i, "");
  if (/^link test\b/i.test(title) || /^link test\b/i.test(stem)) {
    return true;
  }
  return false;
}

export function resolveSpaceForPath(path: string, title: string): VaultSpaceConfig {
  if (isSystemNoiseNote(path, title)) {
    return VAULT_SYSTEM_BUCKET;
  }
  for (const space of VAULT_SPACES) {
    if (space.prefix && path.startsWith(space.prefix)) {
      return space;
    }
  }
  for (const custom of loadCustomVaultSpaces()) {
    if (path.startsWith(custom.prefix)) {
      return (
        customSpacesAsVaultConfig().find((space) => space.id === custom.id) ?? {
          id: custom.id,
          prefix: custom.prefix,
          label: custom.label,
          sort: 20,
          icon: "folder",
          filterChip: true,
          alwaysShow: true,
        }
      );
    }
  }
  return VAULT_OTHER_SPACE;
}

export function loadShowSystemNotes(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(SHOW_SYSTEM_NOTES_KEY) === "true";
}

export function saveShowSystemNotes(value: boolean): void {
  localStorage.setItem(SHOW_SYSTEM_NOTES_KEY, value ? "true" : "false");
}

export function loadLastSpace(): string | null {
  if (typeof localStorage === "undefined") return null;
  const value = localStorage.getItem(LAST_SPACE_KEY);
  if (!value || value === "system_bucket") return null;
  return getSpaceById(value) ? value : null;
}

export function saveLastSpace(spaceId: string | null): void {
  if (typeof localStorage === "undefined") return;
  if (!spaceId || spaceId === "system_bucket" || spaceId === "other") {
    localStorage.removeItem(LAST_SPACE_KEY);
    return;
  }
  localStorage.setItem(LAST_SPACE_KEY, spaceId);
}

export function countNotesBySpace(
  notes: { path: string; title: string }[],
  showSystemNotes: boolean,
): Map<string, number> {
  const counts = new Map<string, number>();
  for (const note of notes) {
    if (shouldHideGarageNote(note.path, note.title, showSystemNotes)) continue;
    const space = resolveSpaceForPath(note.path, note.title);
    counts.set(space.id, (counts.get(space.id) ?? 0) + 1);
  }
  return counts;
}

export function getSpaceById(id: string): VaultSpaceConfig | undefined {
  return (
    VAULT_SPACES.find((space) => space.id === id) ??
    customSpacesAsVaultConfig().find((space) => space.id === id) ??
    (id === VAULT_OTHER_SPACE.id ? VAULT_OTHER_SPACE : undefined) ??
    (id === VAULT_SYSTEM_BUCKET.id ? VAULT_SYSTEM_BUCKET : undefined)
  );
}

export function creatableVaultSpaces(): VaultSpaceConfig[] {
  return allVaultSpaces().filter(
    (space) => space.id !== "system_bucket" && space.id !== "other",
  );
}
