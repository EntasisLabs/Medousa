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
