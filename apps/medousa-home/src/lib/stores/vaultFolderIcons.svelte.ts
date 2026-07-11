/** Persisted custom icons for vault folders and space groups. */

import {
  isAllowedSurfaceIcon,
  type AllowedSurfaceIcon,
} from "$lib/utils/environmentIconCatalog";

const FOLDER_ICONS_KEY = "medousa-home-vault-folder-icons";

function normalizePrefix(prefix: string): string {
  const trimmed = prefix.trim().replace(/^\/+/, "").replace(/\\/g, "/");
  if (!trimmed) return "";
  return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
}

export function folderIconStorageKey(options: {
  dropPrefix?: string | null;
  spaceId?: string | null;
}): string | null {
  const prefix = options.dropPrefix ? normalizePrefix(options.dropPrefix) : "";
  if (prefix) return prefix;
  if (options.spaceId) return `space:${options.spaceId}`;
  return null;
}

function loadFolderIcons(): Record<string, AllowedSurfaceIcon> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(FOLDER_ICONS_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, string>;
    if (!parsed || typeof parsed !== "object") return {};
    const next: Record<string, AllowedSurfaceIcon> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (typeof key === "string" && isAllowedSurfaceIcon(value)) {
        next[key] = value;
      }
    }
    return next;
  } catch {
    return {};
  }
}

function saveFolderIcons(icons: Record<string, AllowedSurfaceIcon>) {
  localStorage.setItem(FOLDER_ICONS_KEY, JSON.stringify(icons));
}

export class VaultFolderIconsStore {
  icons = $state<Record<string, AllowedSurfaceIcon>>(loadFolderIcons());

  get(key: string | null | undefined): AllowedSurfaceIcon | null {
    if (!key) return null;
    return this.icons[key] ?? null;
  }

  set(key: string, icon: AllowedSurfaceIcon) {
    this.icons = { ...this.icons, [key]: icon };
    saveFolderIcons(this.icons);
  }

  clear(key: string) {
    if (!(key in this.icons)) return;
    const next = { ...this.icons };
    delete next[key];
    this.icons = next;
    saveFolderIcons(this.icons);
  }
}

export const vaultFolderIcons = new VaultFolderIconsStore();
