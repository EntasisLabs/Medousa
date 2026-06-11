export interface VaultNote {
  path: string;
  title: string;
  byte_size: number;
  content_hash: string;
  modified_at_utc: string;
  created_at_utc: string;
  tags: string[];
  wikilinks_out: string[];
  backlinks: string[];
  kind?: string;
}

export interface VaultNoteSummary {
  path: string;
  title: string;
  modified_at_utc: string;
  kind?: string;
}

export interface VaultNotesListResponse {
  notes: VaultNote[];
}

export interface VaultNoteContentResponse {
  note: VaultNote;
  content: string;
}

export interface VaultWriteResponse {
  note: VaultNote;
  created: boolean;
}

export interface VaultSearchHit {
  note: VaultNoteSummary;
  score: number;
  matched_terms: string[];
  snippet?: string | null;
}

export interface VaultSearchResponse {
  query: string;
  hits: VaultSearchHit[];
}

export interface VaultBacklinksResponse {
  path: string;
  backlinks: string[];
}

export interface VaultTreeNode {
  name: string;
  path: string | null;
  title?: string | null;
  kind?: string | null;
  displayLabel?: string | null;
  spaceId?: string | null;
  noteCount?: number;
  defaultCollapsed?: boolean;
  /** Folder prefix when dragging notes onto this tree row. */
  dropPrefix?: string | null;
  children: VaultTreeNode[];
  isFolder: boolean;
}
