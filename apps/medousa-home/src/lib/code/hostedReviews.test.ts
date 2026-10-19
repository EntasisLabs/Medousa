/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ query: vi.fn(), providers: vi.fn() }));
vi.mock("./hostedReviews", () => ({ listReviewProviders: api.providers, queryHostedReviews: api.query, hostedReviewKey: (ref: unknown) => JSON.stringify(ref) }));
import { HostedReviews } from "./hostedReviews.svelte";
const provider = { protocol_version: 1, id: "example", label: "Example", hosts: ["git.example.test"], operations: ["resolve", "list"] };
function review(id: string, updated = "2026-10-18") {
  return { reference: { provider_id: "example", host: "git.example.test", repository: "team/repo", native_id: id }, title: id, body: "Large description", updated_at: updated };
}
beforeEach(() => { vi.clearAllMocks(); localStorage.clear(); });
describe("hosted review queue", () => {
  it("discards a late response after the workshop changes", async () => {
    const queue = new HostedReviews(); queue.reset("local"); queue.providers = [provider];
    let finish!: (value: unknown) => void; api.query.mockImplementation(() => new Promise((resolve) => finish = resolve));
    const pending = queue.open("https://git.example.test/team/repo/pull/1"); queue.reset("remote");
    finish({ kind: "review", review: review("1") }); expect(await pending).toBeNull(); expect(queue.reviews).toEqual([]); expect(queue.loading).toBe(false);
  });
  it("uses the chosen package and retains metadata rather than bodies", async () => {
    const queue = new HostedReviews(); queue.reset("local"); queue.providers = [provider];
    api.query.mockResolvedValue({ kind: "reviews", reviews: [review("new", "2026-10-18"), review("old", "2026-10-17")], next_cursor: "2" });
    await queue.open("team/repo"); expect(api.query).toHaveBeenCalledWith("example", "git.example.test", { operation: "list", repository: "team/repo", cursor: null }, null);
    expect(queue.reviews.map((r) => r.title)).toEqual(["new", "old"]); expect(queue.reviews[0].body).toBe(""); expect(queue.nextCursor).toBe("2");
  });
  it("deduplicates pagination and preserves provider errors", async () => {
    const queue = new HostedReviews(); queue.reset("local"); queue.providers = [provider];
    api.query.mockResolvedValueOnce({ kind: "reviews", reviews: [review("1")], next_cursor: "2" }).mockRejectedValueOnce(new Error("Sign in on this workshop"));
    await queue.open("team/repo"); await queue.more(); expect(queue.reviews).toHaveLength(1); expect(queue.error).toBe("Sign in on this workshop"); expect(queue.nextCursor).toBe("2");
  });
});

it("does not discard package discovery when a query starts before discovery completes", async () => {
  const queue = new HostedReviews(); queue.reset("local");
  let finish!: (value: unknown) => void;
  api.providers.mockImplementation(() => new Promise((resolve) => finish = resolve));
  const pending = queue.discover(); await queue.open("team/repo");
  finish({ providers: [provider], problems: [] }); await pending;
  expect(queue.providers).toEqual([provider]);
});

it("pins repository requests to the selected account and drops late responses after account changes", async () => {
  const queue = new HostedReviews(); queue.reset("workshop"); queue.providers = [provider];
  queue.selectAccount("account-a", "example", "git.example.test");
  let finish!: (value: unknown) => void;
  api.query.mockImplementationOnce(() => new Promise((resolve) => finish = resolve));
  const pending = queue.open("team/repo");
  expect(api.query).toHaveBeenLastCalledWith("example", "git.example.test", {operation:"list",repository:"team/repo",cursor:null}, "account-a");
  queue.selectAccount("account-b", "example", "git.example.test");
  finish({kind:"reviews",reviews:[review("other-account")],next_cursor:null});
  await pending; expect(queue.reviews).toEqual([]);
  const restored = new HostedReviews(); restored.reset("workshop"); expect(restored.connectionId).toBe("account-b");
  restored.reset("another-workshop"); expect(restored.connectionId).toBeNull();
});

it("replaces inbox folders, paginates within the account, and rejects late folder responses", async () => {
  const queue = new HostedReviews(); queue.reset("workshop"); queue.providers = [{ ...provider, operations: [...provider.operations, "inbox"] }]; queue.selectAccount("account-a", "example", "git.example.test");
  api.query.mockResolvedValueOnce({kind:"reviews",reviews:[review("requested")],next_cursor:"2"}); await queue.inbox("review_requested"); expect(queue.reviews[0].title).toBe("requested");
  api.query.mockResolvedValueOnce({kind:"reviews",reviews:[review("another")],next_cursor:null}); await queue.more(); expect(api.query).toHaveBeenLastCalledWith("example", "git.example.test", {operation:"inbox",folder:"review_requested",cursor:"2"}, "account-a");
  let finish!: (value: unknown) => void; api.query.mockImplementationOnce(() => new Promise(resolve => finish=resolve)); const old=queue.inbox("authored"); api.query.mockResolvedValueOnce({kind:"reviews",reviews:[review("participating")],next_cursor:null}); await queue.inbox("participating"); finish({kind:"reviews",reviews:[review("stale")],next_cursor:null}); await old;
  expect(queue.reviews.map(r=>r.title)).toEqual(["participating"]); expect(queue.folder).toBe("participating");
});
