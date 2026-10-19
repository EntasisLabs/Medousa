import type { HostedReviewRef } from "$lib/types/generated/daemon_api";

/** Only unfinished text/location metadata; never cache descriptions or patches.
 * sessionStorage keeps drafts across tab mounts within this window. */
export function reviewDraftKey(scope: string, reference: HostedReviewRef, connectionId: string | null, part: string): string {
  return `medousa.review-draft:${JSON.stringify([scope, reference.provider_id, reference.host, reference.repository, reference.native_id, connectionId, part])}`;
}
export function readReviewDraft(key: string): Record<string, unknown> | null {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(key) || "null");
    return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
  } catch { return null; }
}
export function saveReviewDraft(key: string, value: unknown) {
  try { if (value === null) sessionStorage.removeItem(key); else sessionStorage.setItem(key, JSON.stringify(value)); }
  catch { /* A full/disabled window store must not prevent using the review. */ }
}
