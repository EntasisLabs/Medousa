export interface ExternalFileEntry {
  path: string;
  name: string;
  is_dir: boolean;
  ext?: string | null;
  modified_at_utc: string;
  size_bytes: number;
}

export interface PinnedRoot {
  id: string;
  path: string;
  label: string;
}

export type LibrarySidebarMode = "vault" | "files" | "presentations";
