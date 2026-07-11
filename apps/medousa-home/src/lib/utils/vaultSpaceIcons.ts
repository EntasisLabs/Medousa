import {
  BookOpen,
  Bug,
  Folder,
  Inbox,
  Layers,
  Settings,
  Wallet,
  type Icon,
} from "@lucide/svelte";
import type { Component } from "svelte";
import {
  VAULT_OTHER_SPACE,
  VAULT_SPACES,
  VAULT_SYSTEM_BUCKET,
} from "$lib/config/vaultSpaces";
import {
  folderIconStorageKey,
  vaultFolderIcons,
} from "$lib/stores/vaultFolderIcons.svelte";
import { loadCustomVaultSpaces } from "$lib/utils/vaultCustomSpaces";
import { environmentIcon } from "$lib/utils/environmentIcons";

function defaultIconForSpace(spaceId: string | null | undefined): Component {
  switch (spaceId) {
    case "journal":
      return BookOpen;
    case "projects":
      return Folder;
    case "finance":
      return Wallet;
    case "inbox":
      return Inbox;
    case "bugs":
      return Bug;
    case "system_bucket":
      return Settings;
    case "other":
      return Layers;
    default:
      if (spaceId?.startsWith("custom_")) return Folder;
      return Layers;
  }
}

function prefixForSpaceId(spaceId: string | null | undefined): string | null {
  if (!spaceId) return null;
  const builtIn = [...VAULT_SPACES, VAULT_OTHER_SPACE, VAULT_SYSTEM_BUCKET].find(
    (space) => space.id === spaceId,
  );
  if (builtIn) return builtIn.prefix || null;
  const custom = loadCustomVaultSpaces().find((space) => space.id === spaceId);
  return custom?.prefix ?? null;
}

export function iconForSpace(spaceId: string | null | undefined): Component {
  const key = folderIconStorageKey({
    dropPrefix: prefixForSpaceId(spaceId),
    spaceId,
  });
  const custom = key ? vaultFolderIcons.get(key) : null;
  if (custom) return environmentIcon(custom) as typeof Icon;
  return defaultIconForSpace(spaceId);
}
