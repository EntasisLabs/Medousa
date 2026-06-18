export interface IdentityPersona {
  persona_id: string;
  display_name: string;
  status: string;
}

export interface IdentityUser {
  user_id: string;
  timezone: string;
  status: string;
  preferences?: Record<string, unknown>;
}

export interface IdentityChannel {
  channel_id: string;
  channel_type: string;
  proactive_allowed: boolean;
  status: string;
}

export interface IdentityContact {
  contact_id: string;
  display_name: string;
}

export interface IdentityRelationship {
  relationship_id: string;
  relationship_kind: string | Record<string, unknown>;
  trust_level: number;
  confidence: number;
}

export interface IdentityClaim {
  claim_id: string;
  summary: string;
  confidence: number;
}

export interface IdentityContextResponse {
  graph_depth_used: number;
  persona?: IdentityPersona | null;
  user?: IdentityUser | null;
  channel?: IdentityChannel | null;
  contacts?: IdentityContact[];
  relationships?: IdentityRelationship[];
  policy_profiles?: Array<{ policy_profile_id: string; graph_max_depth: number }>;
  flattened_claims?: IdentityClaim[];
}

export interface IdentityRememberRequest {
  user_id?: string | null;
  fact_kind: "preference" | "person" | "note" | string;
  subject: string;
  statement: string;
  attributes?: string[];
  source?: string | null;
}

export interface IdentityRememberResponse {
  committed: boolean;
  requires_confirmation: boolean;
  proposal_ids: string[];
  digest_preview?: string | null;
  message: string;
}

export interface IdentityDigestPreviewResponse {
  digest_text: string;
  preference_count: number;
  contact_count: number;
  relationship_count: number;
  claim_count: number;
}

export interface IdentityExportMarkdownResponse {
  export_dir: string;
  files: string[];
}
