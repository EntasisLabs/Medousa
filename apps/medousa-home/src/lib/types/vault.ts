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
}

export interface VaultNoteSummary {
  path: string;
  title: string;
  modified_at_utc: string;
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
  children: VaultTreeNode[];
  isFolder: boolean;
}
