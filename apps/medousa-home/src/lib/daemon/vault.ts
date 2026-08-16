import { invoke } from "@tauri-apps/api/core";
import type {
  VaultBacklinksResponse,
  VaultFileContentResponse,
  VaultNoteContentResponse,
  VaultNotesListResponse,
  VaultChangesResponse,
  VaultRootsResponse,
  VaultTagsListResponse,
  VaultSearchResponse,
  VaultWriteResponse,
} from "$lib/types/vault";

export async function listVaultNotes(options?: {
  prefix?: string;
  limit?: number;
  tags?: string[];
  tagPrefix?: string;
  cursor?: string;
  generation?: number;
}): Promise<VaultNotesListResponse> {
  const tags =
    options?.tags?.map((tag) => tag.trim()).filter(Boolean).join(",") || undefined;
  return invoke<VaultNotesListResponse>("vault_list_notes", {
    prefix: options?.prefix,
    limit: options?.limit,
    tags,
    tagPrefix: options?.tagPrefix,
    cursor: options?.cursor,
    generation: options?.generation,
  });
}

export async function listVaultChanges(options?: {
  sinceGeneration?: number;
  cursor?: string;
  limit?: number;
}): Promise<VaultChangesResponse> {
  return invoke<VaultChangesResponse>("vault_list_changes", {
    sinceGeneration: options?.sinceGeneration,
    cursor: options?.cursor,
    limit: options?.limit,
  });
}

export async function listVaultTags(options?: {
  prefix?: string;
  limit?: number;
}): Promise<VaultTagsListResponse> {
  return invoke<VaultTagsListResponse>("vault_list_tags", {
    prefix: options?.prefix,
    limit: options?.limit,
  });
}

export async function getVaultNote(
  path: string,
): Promise<VaultNoteContentResponse> {
  return invoke<VaultNoteContentResponse>("vault_get_note", { path });
}

export async function getVaultFile(
  path: string,
): Promise<VaultFileContentResponse> {
  return invoke<VaultFileContentResponse>("vault_get_file", { path });
}

export async function saveVaultNote(
  path: string,
  content: string,
  options?: {
    contentHash?: string;
    sessionId?: string;
    autoWorkshopTags?: boolean;
  },
): Promise<VaultWriteResponse> {
  return invoke<VaultWriteResponse>("vault_save_note", {
    path,
    content,
    contentHash: options?.contentHash,
    sessionId: options?.sessionId,
    autoWorkshopTags: options?.autoWorkshopTags,
  });
}

export async function createVaultNote(
  path: string,
  content: string,
  options?: {
    sessionId?: string;
    semanticTags?: string[];
    autoWorkshopTags?: boolean;
  },
): Promise<VaultWriteResponse> {
  return invoke<VaultWriteResponse>("vault_create_note", {
    path,
    content,
    sessionId: options?.sessionId,
    semanticTags: options?.semanticTags,
    autoWorkshopTags: options?.autoWorkshopTags,
  });
}

export async function deleteVaultNote(path: string): Promise<{ path: string; deleted: boolean }> {
  return invoke<{ path: string; deleted: boolean }>("vault_delete_note", { path });
}

export async function searchVaultNotes(
  query: string,
  limit?: number,
  tags?: string[],
): Promise<VaultSearchResponse> {
  const tagFilter =
    tags?.map((tag) => tag.trim()).filter(Boolean).join(",") || undefined;
  return invoke<VaultSearchResponse>("vault_search", {
    query,
    limit,
    tags: tagFilter,
  });
}

export async function getVaultBacklinks(
  path: string,
): Promise<VaultBacklinksResponse> {
  return invoke<VaultBacklinksResponse>("vault_backlinks", { path });
}

export async function listVaultRoots(): Promise<VaultRootsResponse> {
  return invoke<VaultRootsResponse>("vault_list_roots");
}

export async function setActiveVaultRoot(rootId: string): Promise<VaultRootsResponse> {
  return invoke<VaultRootsResponse>("vault_set_active_root", { rootId });
}

export async function addVaultRoot(
  label: string,
  path: string,
  id?: string,
): Promise<VaultRootsResponse> {
  return invoke<VaultRootsResponse>("vault_add_root", { label, path, id });
}

export type VaultTrashEntry = {
  path: string;
  trashedAt?: string | null;
};

export async function listVaultTrash(limit?: number): Promise<{ entries: VaultTrashEntry[] }> {
  return invoke("vault_list_trash", { limit });
}

export async function restoreVaultTrash(
  path: string,
): Promise<{ path: string; restored: boolean }> {
  return invoke("vault_restore_trash", { path });
}

export type VaultGitDetect = {
  available: boolean;
  path?: string | null;
  version?: string | null;
  enabled: boolean;
  platformHint: string;
};

export type VaultGitStatus = {
  enabled: boolean;
  available: boolean;
  isRepo: boolean;
  branch?: string | null;
  dirtyCount: number;
  vaultRoot: string;
  gitPath?: string | null;
};

export type VaultGitLogEntry = {
  id: string;
  shortId: string;
  message: string;
  author: string;
  committedAt: string;
};

export async function vaultGitDetect(): Promise<VaultGitDetect> {
  return invoke<VaultGitDetect>("vault_git_detect");
}

export async function vaultGitStatus(): Promise<VaultGitStatus> {
  return invoke<VaultGitStatus>("vault_git_status");
}

export async function vaultGitEnable(
  enabled: boolean,
  initIfNeeded = false,
): Promise<{ enabled: boolean; status: VaultGitStatus }> {
  return invoke("vault_git_enable", { enabled, initIfNeeded });
}

export async function vaultGitInit(): Promise<VaultGitStatus> {
  return invoke<VaultGitStatus>("vault_git_init");
}

export async function vaultGitInstall(): Promise<VaultGitDetect> {
  return invoke<VaultGitDetect>("vault_git_install");
}

export async function vaultGitLog(options?: {
  path?: string;
  limit?: number;
}): Promise<VaultGitLogEntry[]> {
  return invoke<VaultGitLogEntry[]>("vault_git_log", {
    path: options?.path,
    limit: options?.limit,
  });
}

export async function vaultGitCommit(
  message: string,
  paths?: string[],
): Promise<{ id: string; message: string }> {
  return invoke("vault_git_commit", { message, paths: paths ?? [] });
}

export async function vaultGitRestore(commit: string, path: string): Promise<void> {
  await invoke("vault_git_restore", { commit, path });
}

export async function vaultGitDiff(
  path: string,
  commit?: string,
): Promise<{ path: string; patch: string }> {
  return invoke("vault_git_diff", { path, commit });
}

export async function vaultGitWorktrees(): Promise<
  Array<{ path: string; head: string; branch?: string | null }>
> {
  return invoke("vault_git_worktrees");
}
