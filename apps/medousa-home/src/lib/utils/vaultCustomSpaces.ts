/** User-defined vault groups (localStorage) — extra top-level spaces beyond built-ins. */

import type { VaultSpaceConfig } from "$lib/config/vaultSpaces";
import { slugifyTitle } from "$lib/utils/vaultTemplates";

const CUSTOM_SPACES_KEY = "medousa-home-vault-custom-spaces";
const MAX_CUSTOM_SPACES = 12;

export interface CustomVaultSpace {
  id: string;
  label: string;
  prefix: string;
}

function normalizePrefix(prefix: string): string {
  const trimmed = prefix.trim().replace(/^\/+/, "").replace(/\\/g, "/");
  if (!trimmed) return "";
  return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
}

export function loadCustomVaultSpaces(): CustomVaultSpace[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(CUSTOM_SPACES_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as CustomVaultSpace[];
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((space) => space?.id && space?.label && space?.prefix)
      .slice(0, MAX_CUSTOM_SPACES)
      .map((space) => ({
        id: space.id,
        label: space.label,
        prefix: normalizePrefix(space.prefix),
      }));
  } catch {
    return [];
  }
}

function saveCustomVaultSpaces(spaces: CustomVaultSpace[]) {
  localStorage.setItem(CUSTOM_SPACES_KEY, JSON.stringify(spaces.slice(0, MAX_CUSTOM_SPACES)));
}

export function addCustomVaultSpace(label: string): CustomVaultSpace | null {
  const trimmed = label.trim();
  if (!trimmed) return null;
  const slug = slugifyTitle(trimmed);
  const id = `custom_${slug}`;
  const prefix = `groups/${slug}/`;
  const existing = loadCustomVaultSpaces();
  if (existing.some((space) => space.id === id || space.prefix === prefix)) {
    return existing.find((space) => space.id === id || space.prefix === prefix) ?? null;
  }
  const created: CustomVaultSpace = { id, label: trimmed, prefix };
  saveCustomVaultSpaces([...existing, created]);
  return created;
}

export function removeCustomVaultSpace(id: string) {
  saveCustomVaultSpaces(loadCustomVaultSpaces().filter((space) => space.id !== id));
}

export function customSpacesAsVaultConfig(): VaultSpaceConfig[] {
  return loadCustomVaultSpaces().map((space, index) => ({
    id: space.id,
    prefix: space.prefix,
    label: space.label,
    sort: 20 + index,
    icon: "folder" as const,
    filterChip: true,
    alwaysShow: true,
  }));
}

export function folderPrefixForSpaceId(spaceId: string, builtInPrefix: string): string {
  const custom = loadCustomVaultSpaces().find((space) => space.id === spaceId);
  return custom?.prefix ?? builtInPrefix;
}
