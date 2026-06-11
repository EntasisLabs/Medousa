export interface LocusAvecSnapshot {
  stability: number;
  friction: number;
  logic: number;
  autonomy: number;
  psi: number;
}

export interface LocusNodeSummary {
  sync_key: string;
  session_id: string;
  tier: string;
  timestamp: string;
  context_summary: string;
  psi: number;
  rho: number;
  kappa: number;
  user_avec?: LocusAvecSnapshot | null;
  model_avec?: LocusAvecSnapshot | null;
}

export interface LocusNodesListResponse {
  retrieved: number;
  nodes: LocusNodeSummary[];
}

export interface LocusNodeDetailResponse {
  node: LocusNodeSummary;
  raw: string;
}

export interface ContextThreadEntry {
  id: string;
  title: string;
  subtitle: string;
  searchText: string;
  sessionId: string;
  tier: string;
  timestamp: string;
  syncKey: string;
}
