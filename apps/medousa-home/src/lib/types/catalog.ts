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

export interface CapabilityListEntry {
  id: string;
  title: string;
  binding_count: number;
}

export interface CapabilityListResponse {
  capabilities: CapabilityListEntry[];
}
