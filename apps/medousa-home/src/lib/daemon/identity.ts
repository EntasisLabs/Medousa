import { invoke } from "@tauri-apps/api/core";
import type {
  IdentityContextResponse,
  IdentityDigestPreviewResponse,
  IdentityExportMarkdownResponse,
  IdentityRememberRequest,
  IdentityRememberResponse,
} from "$lib/types/identity";

export async function getIdentityContext(request: {
  user_id?: string;
  persona_id?: string;
  channel_id?: string;
  policy_profile?: string;
  relationship_limit?: number;
  mode?: string;
}): Promise<IdentityContextResponse> {
  return invoke<IdentityContextResponse>("identity_get_context", { request });
}

export async function rememberIdentityFact(
  request: IdentityRememberRequest,
): Promise<IdentityRememberResponse> {
  return invoke("identity_remember", { request });
}

export async function getIdentityDigestPreview(request?: {
  user_id?: string;
  relationship_limit?: number;
  mode?: string;
}): Promise<IdentityDigestPreviewResponse> {
  return invoke("identity_digest_preview", {
    request: {
      mode: request?.mode ?? "cognitive",
      relationship_limit: request?.relationship_limit ?? 32,
      user_id: request?.user_id ?? null,
      persona_id: null,
      channel_id: null,
      policy_profile: null,
    },
  });
}

export async function exportIdentityMarkdown(request?: {
  user_id?: string;
  dir?: string | null;
}): Promise<IdentityExportMarkdownResponse> {
  return invoke("identity_export_markdown", {
    request: {
      user_id: request?.user_id ?? null,
      dir: request?.dir ?? null,
    },
  });
}

export async function exportUserProfileBundle(request: {
  profileId: string;
  sessionLimit?: number;
  nodeLimitPerSession?: number;
}): Promise<{ bundle: unknown }> {
  return invoke("identity_export_profile", {
    profileId: request.profileId,
    sessionLimit: request.sessionLimit ?? null,
    nodeLimitPerSession: request.nodeLimitPerSession ?? null,
  });
}

export async function importUserProfileBundle(request: {
  bundle: unknown;
  dryRun?: boolean;
}): Promise<{
  dry_run: boolean;
  profile_id: string;
  created_profile: boolean;
  message: string;
}> {
  return invoke("identity_import_profile", {
    bundle: request.bundle,
    dryRun: request.dryRun ?? false,
  });
}

export async function listUserProfiles(): Promise<
  import("$lib/types/userProfile").ListUserProfilesResponse
> {
  return invoke("identity_list_profiles");
}

export async function createUserProfile(
  slug: string,
  displayName: string,
): Promise<import("$lib/types/userProfile").CreateUserProfileResponse> {
  return invoke("identity_create_profile", {
    slug,
    displayName,
  });
}

export async function setActiveUserProfile(
  profileId: string,
): Promise<import("$lib/types/userProfile").SetActiveUserProfileResponse> {
  return invoke("identity_set_active_profile", { profileId });
}
