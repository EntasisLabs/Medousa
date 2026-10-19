/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { HostedReview, ReviewInlineComment } from "$lib/types/generated/daemon_api";
const api = vi.hoisted(() => ({ mutate: vi.fn() }));
vi.mock("$lib/code/hostedReviews", () => ({ mutateHostedReview: api.mutate }));
import Actions from "./HostedReviewActions.svelte";
const reference = { provider_id: "example", host: "git.example.test", repository: "team/repo", native_id: "opaque" };
const review: HostedReview = { reference, title: "Fix", body: "Description", author: "author", state: "open", native_state: "open", head_revision: "abc", base_revision: "def", head_branch: "feature", base_branch: "main", url: "https://git.example.test/pr/1", updated_at: "2026-10-18", additions: 1, deletions: 0, changed_files: 1, actions: ["comment", "submit_review", "approve", "request_changes", "merge", "edit", "request_reviewers", "close"], requested_reviewers: [], merge_state: "clean", merge_methods: ["squash"] };
let component: ReturnType<typeof mount> | null = null;
const refreshed = vi.fn(async () => {}), cleared = vi.fn();
async function settle() { for (let i = 0; i < 8; i++) { await Promise.resolve(); flushSync(); } }
function click(text: string) {
  let button = [...document.querySelectorAll("button")].find(b => b.textContent?.trim() === text) as HTMLButtonElement | undefined;
  if (!button) { document.querySelector<HTMLButtonElement>("[aria-label='Pull request actions']")!.click(); flushSync(); button = [...document.querySelectorAll("button")].find(b => b.textContent?.trim() === text) as HTMLButtonElement | undefined; }
  button!.click();
}
function fill(selector: string, value: string) { const input = document.querySelector<HTMLInputElement>(selector)!; input.value = value; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); }
function submit() { document.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); }
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); sessionStorage.clear(); vi.clearAllMocks(); });
function render(drafts: ReviewInlineComment[] = [], draftRevision: string | null = null) { component = mount(Actions, { target: document.body, props: { review, connectionId: "account-a", drafts, draftRevision, onupdated: refreshed, onclear: cleared } }); flushSync(); }
it("posts comments through the selected account and refreshes after confirmation", async () => {
  api.mutate.mockResolvedValue({ kind: "updated", message: "Comment posted." }); render(); click("Comment"); flushSync(); fill("textarea", "Actual comment"); submit(); await settle();
  expect(api.mutate).toHaveBeenCalledWith("example", "git.example.test", { operation: "comment", review: reference, body: "Actual comment", reply_to: null }, "account-a"); expect(refreshed).toHaveBeenCalledOnce(); expect(document.body.textContent).toContain("Comment posted.");
});
it("submits decisions and staged comments against the reviewed commit", async () => {
  api.mutate.mockResolvedValue({ kind: "updated", message: "Review submitted." }); const comments: ReviewInlineComment[] = [{ path: "file.rs", line: 4, side: "old", body: "Needs work" }]; render(comments, "abc"); click("Review (1)"); flushSync();
  const select = document.querySelector<HTMLSelectElement>("[aria-label='Review decision']")!; select.value = "request_changes"; select.dispatchEvent(new Event("change", { bubbles: true })); flushSync(); fill("textarea", "Please revise"); submit(); await settle();
  expect(api.mutate.mock.calls[0][2]).toEqual({ operation: "submit_review", review: reference, revision: "abc", decision: "request_changes", body: "Please revise", comments }); expect(cleared).toHaveBeenCalledOnce();
});
it("retains drafts on an uncertain write and never automatically resubmits", async () => {
  api.mutate.mockRejectedValue(new Error("Connection interrupted")); render(); click("Comment"); flushSync(); fill("textarea", "Keep this draft"); submit(); await settle(); expect(document.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe("Keep this draft"); expect(document.body.textContent).toContain("Refresh to check the result"); expect(api.mutate).toHaveBeenCalledOnce(); expect(refreshed).not.toHaveBeenCalled();
});
it("requires an explicit merge submission and pins the expected head", async () => {
  api.mutate.mockResolvedValue({ kind: "updated", message: "Merged." }); render(); click("Merge…"); flushSync(); expect(api.mutate).not.toHaveBeenCalled(); expect(document.body.textContent).toContain("Merge into main"); submit(); await settle(); expect(api.mutate.mock.calls[0][2]).toEqual({ operation: "merge", review: reference, revision: "abc", method: "squash" });
});
it("does not move old draft comments onto a refreshed commit", async () => {
  render([{ path: "file.rs", line: 4, side: "new", body: "Old revision" }], "older"); click("Review (1)"); flushSync(); expect(document.querySelector<HTMLButtonElement>("button[type='submit']")?.disabled).toBe(true); submit(); await settle(); expect(api.mutate).not.toHaveBeenCalled();
});
it("restores unfinished text when the review tab remounts", async () => {
  render(); click("Comment"); flushSync(); fill("textarea", "Finish this after checking another PR");
  await unmount(component!); component = null; document.body.replaceChildren(); render(); await settle();
  expect(document.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe("Finish this after checking another PR"); expect(api.mutate).not.toHaveBeenCalled();
});
