import { afterEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: api.invoke }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => "workshop" }));
import { mutateHostedReview } from "./hostedReviews";
import { hostedReviewCache } from "./hostedReviewCache";
const context = { scope: "workshop", provider: "example", host: "git.example.test", connectionId: "account" };
const reference = { provider_id: context.provider, host: context.host, repository: "team/repo", native_id: "1" };
const request = { operation: "discussion" as const, review: reference, cursor: null };
afterEach(() => { hostedReviewCache.clear(); vi.clearAllMocks(); });

it.each([false, true])("invalidates cached reads after a write attempt (transport failed=%s)", async failed => {
  await hostedReviewCache.query(context, request, async () => ({ kind: "discussion", entries: [] }));
  if (failed) api.invoke.mockRejectedValueOnce(new Error("timeout"));
  else api.invoke.mockResolvedValueOnce({ protocol_version: 1, outcome: { status: "ok", result: { kind: "updated", message: "Saved" } } });
  const mutation = mutateHostedReview(context.provider, context.host, { operation: "comment", review: reference, body: "Comment", reply_to: null }, context.connectionId);
  if (failed) await expect(mutation).rejects.toThrow("timeout"); else await mutation;
  expect(hostedReviewCache.peek(context, request)).toBeNull();
});
