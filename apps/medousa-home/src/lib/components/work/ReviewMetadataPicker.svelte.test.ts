/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
import type { HostedReview } from "$lib/types/generated/daemon_api";
const api = vi.hoisted(() => ({ mutate: vi.fn() }));
vi.mock("$lib/code/hostedReviews", () => ({ mutateHostedReview: api.mutate }));
vi.mock("$lib/utils/browserPopoverOverlay", () => ({ pushBrowserPopoverOverlay: vi.fn(), popBrowserPopoverOverlay: vi.fn() }));
import ReviewMetadataPicker from "./ReviewMetadataPicker.svelte";
let component: ReturnType<typeof mount> | null = null;
const reference = { provider_id: "example", host: "git.example", repository: "team/repo", native_id: "opaque" };
const review = { reference, author: "owner", actions: ["add_labels", "remove_label", "request_reviewers", "remove_reviewers"], labels: [{ id: "bug", name: "Bug", description: "Fix needed", color: "ff0000" }], requested_reviewers: ["reviewer"] } as HostedReview;
async function settle() { for (let i = 0; i < 16; i++) { await Promise.resolve(); flushSync(); } }
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); vi.clearAllMocks(); });
it("searches labels, preserves opaque IDs and sends incremental changes", async () => {
  const query = vi.fn().mockResolvedValue({ kind: "labels", labels: [{ id: "docs/id", name: "Docs", description: "Documentation" }], next_cursor: null });
  const onupdated = vi.fn().mockResolvedValue(undefined);
  api.mutate.mockResolvedValue({ kind: "updated", message: "Saved" });
  component = mount(ReviewMetadataPicker, { target: document.body, props: { kind: "labels", review, connectionId: "account", query, onupdated } });
  document.querySelector<HTMLButtonElement>('[aria-label="Edit labels"]')!.click(); await settle();
  expect(query).toHaveBeenCalledWith({ operation: "labels", review: reference, cursor: null });
  const search = document.querySelector<HTMLInputElement>('input')!; search.value = "Documentation"; search.dispatchEvent(new Event("input", { bubbles: true })); await settle();
  expect(document.querySelectorAll('.option')).toHaveLength(1);
  document.querySelector<HTMLButtonElement>('.option')!.click(); await settle();
  expect(api.mutate).toHaveBeenCalledWith("example", "git.example", { operation: "add_labels", review: reference, labels: ["docs/id"] }, "account");
  expect(onupdated).toHaveBeenCalledTimes(1);
  search.value = "Bug"; search.dispatchEvent(new Event("input", { bubbles: true })); await settle();
  document.querySelector<HTMLButtonElement>('.option')!.click(); await settle();
  expect(api.mutate.mock.calls.at(-1)?.[2]).toEqual({ operation: "remove_label", review: reference, label: "bug" });
});
it("excludes the author and removes only pending review requests", async () => {
  const query = vi.fn().mockResolvedValue({ kind: "people", people: [{ id: "owner", name: "Owner" }, { id: "reviewer", name: "Reviewer" }, { id: "new", name: "New person" }], next_cursor: "page2" });
  api.mutate.mockResolvedValue({ kind: "updated", message: "Saved" });
  component = mount(ReviewMetadataPicker, { target: document.body, props: { kind: "reviewers", review, connectionId: null, query, onupdated: vi.fn() } });
  document.querySelector<HTMLButtonElement>('[aria-label="Choose reviewers"]')!.click(); await settle();
  expect(document.querySelectorAll('.option')).toHaveLength(2);
  document.querySelector<HTMLButtonElement>('.option[aria-pressed="true"]')!.click(); await settle();
  expect(api.mutate.mock.calls[0][2]).toEqual({ operation: "remove_reviewers", review: reference, reviewers: ["reviewer"] });
  document.querySelector<HTMLButtonElement>('.more')!.click(); await settle();
  expect(query).toHaveBeenLastCalledWith({ operation: "reviewer_candidates", review: reference, cursor: "page2" });
});
it("doesn't report success or silently retry when a mutation is uncertain", async () => {
  api.mutate.mockRejectedValue(new Error("offline"));
  const onupdated = vi.fn();
  component = mount(ReviewMetadataPicker, { target: document.body, props: { kind: "labels", review, connectionId: null, query: vi.fn().mockResolvedValue({ kind: "labels", labels: [], next_cursor: null }), onupdated } });
  document.querySelector<HTMLButtonElement>('[aria-label="Edit labels"]')!.click(); await settle();
  document.querySelector<HTMLButtonElement>('.option')!.click(); await settle();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Refresh the review");
  expect(document.querySelector('.option')?.getAttribute('aria-pressed')).toBe("true");
  expect(api.mutate).toHaveBeenCalledTimes(1); expect(onupdated).not.toHaveBeenCalled();
});
