import { userProfiles } from "$lib/stores/userProfiles.svelte";
import type { IdentityRememberRequest } from "$lib/types/identity";

/** Attach active profile user id when the daemon has resolved one. */
export function withIdentityUserId(
  request: IdentityRememberRequest,
): IdentityRememberRequest {
  const userId = userProfiles.resolvedUserId;
  return userId ? { ...request, user_id: userId } : request;
}

/** Best-effort parse of free-text teaching into identity remember fields. */
export function parseIdentityTeachInput(text: string): IdentityRememberRequest {
  const trimmed = text.trim();
  if (!trimmed) {
    return { fact_kind: "note", subject: "note", statement: "" };
  }

  const personMatch = trimmed.match(/^([A-Za-z][\w\s'.-]{0,40}?)\s+is\s+my\s+(.+)$/i);
  if (personMatch) {
    return {
      fact_kind: "person",
      subject: personMatch[1].trim(),
      statement: personMatch[2].trim(),
      source: "user_direct",
    };
  }

  const tzMatch = trimmed.match(
    /(?:my\s+)?(?:timezone|time zone)\s*(?:is|:)\s*([A-Za-z0-9_/+-]+)/i,
  );
  if (tzMatch) {
    return {
      fact_kind: "preference",
      subject: "timezone",
      statement: tzMatch[1].trim(),
      source: "user_direct",
    };
  }

  const preferMatch = trimmed.match(/^I\s+(?:prefer|like)\s+(.+)$/i);
  if (preferMatch) {
    return {
      fact_kind: "preference",
      subject: "preference",
      statement: preferMatch[1].trim(),
      source: "user_direct",
    };
  }

  const callMeMatch = trimmed.match(/^call\s+me\s+(.+)$/i);
  if (callMeMatch) {
    return {
      fact_kind: "preference",
      subject: "display_name",
      statement: callMeMatch[1].trim(),
      source: "user_direct",
    };
  }

  return {
    fact_kind: "note",
    subject: trimmed.slice(0, 48),
    statement: trimmed,
    source: "user_direct",
  };
}

export function preferenceDisplayValue(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}
