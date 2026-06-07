export interface ManuscriptScriptEntry {
  relative_path: string;
  risk_class: string;
}

export interface ManuscriptCatalogEntry {
  id: string;
  name: string;
  description?: string | null;
  scope: string;
  path: string;
  has_scripts: boolean;
  scripts: ManuscriptScriptEntry[];
  openshell_enabled: boolean;
}

export interface ManuscriptCatalogResponse {
  count: number;
  manuscripts: ManuscriptCatalogEntry[];
}

export interface CapabilityBindingSummary {
  source: string;
  reference: string;
  available: boolean;
  effect_class?: string | null;
  invoke_via?: string | null;
}

export interface CapabilityListEntry {
  id: string;
  title: string;
  binding_count: number;
  description?: string | null;
  domain?: string;
  has_grapheme?: boolean;
  has_mcp?: boolean;
  bindings_summary?: CapabilityBindingSummary[];
}

export interface CapabilityListResponse {
  capabilities: CapabilityListEntry[];
}

export interface CapabilityBinding {
  source: string;
  reference: string;
  priority: number;
  available: boolean;
  unavailable_reason?: string | null;
  invoke_via?: string | null;
  effect_class?: string | null;
}

export interface CapabilityImplementations {
  grapheme: CapabilityBinding[];
  mcp: CapabilityBinding[];
}

export interface CapabilityRecommendation {
  source: string;
  reference: string;
  reason: string;
}

export interface CapabilityResolveResponse {
  capability: string;
  title: string;
  description?: string | null;
  implementations: CapabilityImplementations;
  recommended?: CapabilityRecommendation | null;
  gateway_unreachable?: boolean | null;
}
