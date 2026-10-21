import { daemonUnary } from "$lib/daemon/contractClient";
import { hostedReviewCache } from "./hostedReviewCache";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import type { HostedReviewRef, ReviewProviderQuery, ReviewProviderResponse, ReviewProviderListResponse, ReviewProviderResult } from "$lib/types/generated/daemon_api";

export type { HostedReview, HostedReviewRef, HostedReviewFile, HostedReviewDiscussionEntry, HostedReviewCheck, ReviewProviderDescriptor } from "$lib/types/generated/daemon_api";
export function hostedReviewKey(reference: HostedReviewRef, connectionId: string | null = null): string {
  return JSON.stringify([reference.provider_id, reference.host, connectionId, reference.repository, reference.native_id]);
}
export const listReviewProviders = () => daemonUnary<ReviewProviderListResponse>("reviews.providers.get");
export async function queryHostedReviews(provider: string, host: string, request: ReviewProviderQuery["request"], connectionId: string | null = null): Promise<ReviewProviderResult> {
  const response = await daemonUnary<ReviewProviderResponse>("reviews.query.post", {}, { provider_id: provider, host, connection_id: connectionId, request });
  if (response.protocol_version !== 1) throw new Error("This workshop uses an unsupported review-provider protocol.");
  if (response.outcome.status === "error") throw new Error(response.outcome.error.message);
  return response.outcome.result;
}

/** Hosted writes use the same provider-neutral contract as SDK callers. */
export async function mutateHostedReview(provider: string, host: string, request: import("$lib/types/generated/daemon_api").ReviewProviderMutation["request"], connectionId: string | null = null): Promise<ReviewProviderResult> {
  if (new TextEncoder().encode(JSON.stringify(request)).length > 120000) throw new Error("This submission is too large. Split the review into smaller comments.");
  // Even a failed transport can have applied a write. Never serve an older
  // cached decision or thread after a submission has been attempted.
  let response: ReviewProviderResponse;
  const context = { provider, host, connectionId, scope: activeWorkshopId() };
  try {
    response = await daemonUnary<ReviewProviderResponse>("reviews.mutate.post", {}, { provider_id: provider, host, connection_id: connectionId, request });
  } finally {
    hostedReviewCache.invalidateReview(context, "review" in request ? request.review : undefined);
  }
  if (response.protocol_version !== 1) throw new Error("This workshop uses an unsupported review-provider protocol.");
  if (response.outcome.status === "error") throw new Error(response.outcome.error.message);
  return response.outcome.result;
}
