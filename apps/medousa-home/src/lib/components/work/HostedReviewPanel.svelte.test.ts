/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const api = vi.hoisted(() => ({ query: vi.fn(), openInBrowser: vi.fn().mockResolvedValue(undefined) }));
vi.mock("$lib/code/hostedReviews", () => ({ queryHostedReviews: api.query }));
vi.mock("$lib/utils/openInBrowser", async (importOriginal) => ({
  ...await importOriginal<typeof import("$lib/utils/openInBrowser")>(),
  openInBrowser: api.openInBrowser,
}));
import HostedReviewPanel from "./HostedReviewPanel.svelte";
import { hostedReviewCache } from "$lib/code/hostedReviewCache";
const reference = { provider_id: "example", host: "git.example.test", repository: "group/repo", native_id: "opaque" };
const review = { reference, title: "A hosted review", body: "", state: "open", author: "author", head_branch: "feature", base_branch: "main", head_revision: "abc", updated_at: "2026-10-18", changed_files: 1, additions: 1, deletions: 0, url: "https://git.example.test/review/opaque" };
let component: ReturnType<typeof mount> | null = null;
async function settle() { for (let i = 0; i < 8; i++) { await Promise.resolve(); flushSync(); } }
function click(label: string) { const button = [...document.querySelectorAll("nav button")].find((b) => b.textContent?.startsWith(label)) as HTMLButtonElement; button.click(); }
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); sessionStorage.clear(); hostedReviewCache.clear(); vi.clearAllMocks(); vi.restoreAllMocks(); });

it("starts with summary, fetches files lazily, and completes a rapid lens switch", async () => {
  let resolveFiles!: (value: unknown) => void;
  api.query.mockImplementation((_provider, _host, input) => {
    if (input.operation === "get") return Promise.resolve({ kind: "review", review });
    if (input.operation === "files") return new Promise((resolve) => resolveFiles = resolve);
    if (input.operation === "checks") return Promise.resolve({ kind: "checks", checks: [{ id: "1", name: "Build", status: "completed", conclusion: "success" }], next_cursor: null });
  });
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle();
  expect(document.body.textContent).toContain("A hosted review"); expect(api.query).toHaveBeenCalledTimes(2);
  expect([...document.querySelectorAll("button")].some((b) => b.textContent?.includes("Approve"))).toBe(false);
  click("Changes"); await settle(); click("Checks"); await settle();
  resolveFiles({ kind: "files", files: [{ path: "file.rs", status: "modified", additions: 1, deletions: 0 }], next_cursor: null }); await settle();
  expect(document.body.textContent).toContain("Build"); expect(api.query.mock.calls.map((call) => call[2].operation)).toEqual(["get", "checks", "files"]);
});

it("does not fetch a patch until a file opens and releases it when closed", async () => {
  let resolvePatch!: (value: unknown) => void;
  api.query.mockImplementation((_provider, _host, input) => {
    if (input.operation === "get") return Promise.resolve({ kind: "review", review });
    if (input.operation === "checks") return Promise.resolve({ kind: "checks", checks: [], next_cursor: null });
    if (input.operation === "files") return Promise.resolve({ kind: "files", files: [{ path: "file.rs", status: "modified", additions: 1, deletions: 0 }], next_cursor: null });
    if (input.operation === "diff") return new Promise((resolve) => resolvePatch = resolve);
  });
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle(); click("Changes"); await settle();
  expect(api.query.mock.calls.map((call) => call[2].operation)).toEqual(["get", "checks", "files"]);
  const file = document.querySelector<HTMLButtonElement>(".file-row")!; file.click(); await settle(); expect(document.body.textContent).toContain("Loading patch…");
  file.click(); await settle(); resolvePatch({ kind: "diff", path: "file.rs", patch: "@@ -1 +1 @@\n-old\n+new", message: null }); await settle();
  expect(file.getAttribute("aria-expanded")).toBe("false"); expect(document.body.textContent).not.toContain("Loading patch…");
});

it("routes description, discussion, check output, and provider links into app browser tabs", async () => {
  api.query.mockImplementation((_provider, _host, input) => {
    if (input.operation === "get") return Promise.resolve({ kind: "review", review: { ...review, body: "[Release](https://example.com/release)" } });
    if (input.operation === "checks") return Promise.resolve({ kind: "checks", checks: [{ id: "1", name: "Build", status: "completed", conclusion: "success", url: "https://example.com/build" }], next_cursor: null });
    if (input.operation === "discussion") return Promise.resolve({ kind: "discussion", entries: [{ id: "1", author: "Reviewer", kind: "comment", body: "[Context](https://example.com/context)", created_at: "2026-10-19", url: "https://example.com/comment" }], next_cursor: null });
    if (input.operation === "check_details") return Promise.resolve({ kind: "check_details", title: "Build", summary: "[Summary](https://example.com/summary)", text: "[Log](https://example.com/log)", annotations: [] });
  });
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle();
  async function follow(url: string) {
    const link = document.querySelector<HTMLAnchorElement>(`a[href="${url}"]`)!;
    expect(link).not.toBeNull();
    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    link.dispatchEvent(event); await settle();
    expect(event.defaultPrevented).toBe(true);
    expect(api.openInBrowser).toHaveBeenLastCalledWith(url, { openedBy: "user", newTab: true });
  }
  await follow("https://example.com/release"); await follow(review.url);
  click("Discussion"); await settle();
  await follow("https://example.com/context"); await follow("https://example.com/comment");
  click("Checks"); await settle();
  await follow("https://example.com/build");
  [...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "Details")!.click(); await settle();
  await follow("https://example.com/summary"); await follow("https://example.com/log");
  expect(api.openInBrowser).toHaveBeenCalledTimes(7);
});

it("reuses a visited review, its checks, file list and patch across tab remounts", async () => {
  api.query.mockImplementation((_provider, _host, input) => {
    if (input.operation === "get") return Promise.resolve({ kind: "review", review });
    if (input.operation === "checks") return Promise.resolve({ kind: "checks", checks: [], next_cursor: null });
    if (input.operation === "files") return Promise.resolve({ kind: "files", files: [{ path: "file.rs", status: "modified", additions: 1, deletions: 1 }], next_cursor: null });
    if (input.operation === "diff") return Promise.resolve({ kind: "diff", path: "file.rs", patch: "@@ -1 +1 @@\n-old\n+new" });
  });
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle();
  click("Changes"); await settle();
  document.querySelector<HTMLButtonElement>(".file-row")!.click(); await settle();
  expect(api.query).toHaveBeenCalledTimes(4);
  await unmount(component); component = null; document.body.replaceChildren();
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle();
  expect(document.querySelector(".file-row")).not.toBeNull();
  document.querySelector<HTMLButtonElement>(".file-row")!.click(); await settle();
  expect(document.body.textContent).not.toContain("Loading patch"); expect(api.query).toHaveBeenCalledTimes(4);
});

it("shows stale content immediately during background refresh and retains it on failure", async () => {
  let time = Date.now(); vi.spyOn(Date, "now").mockImplementation(() => time);
  api.query.mockImplementation((_provider, _host, input) => Promise.resolve(input.operation === "get" ? { kind: "review", review } : { kind: "checks", checks: [{ id: "1", name: "Build", status: "completed", conclusion: "success" }], next_cursor: null }));
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle();
  await unmount(component); component = null; document.body.replaceChildren(); time += 61_000;
  let fail!: (error: Error) => void;
  api.query.mockImplementation((_provider, _host, input) => input.operation === "get" ? new Promise((_resolve, reject) => fail = reject) : Promise.resolve({ kind: "checks", checks: [], next_cursor: null }));
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle();
  expect(document.body.textContent).toContain(review.title); expect(document.body.textContent).toContain("Build");
  expect(document.body.textContent).not.toContain("Loading review"); expect(document.body.textContent).not.toContain("Loading checks");
  fail(new Error("offline")); await settle();
  expect(document.body.textContent).toContain(review.title); expect(document.body.textContent).toContain("offline");
});

it("explicit refresh bypasses cache without blanking the current review", async () => {
  api.query.mockImplementation((_provider, _host, input) => Promise.resolve(input.operation === "get" ? { kind: "review", review } : { kind: "checks", checks: [], next_cursor: null }));
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle();
  let finish!: (value: unknown) => void;
  api.query.mockImplementationOnce(() => new Promise(resolve => finish = resolve));
  document.querySelector<HTMLButtonElement>('button[aria-label="Refresh review"]')!.click(); await settle();
  expect(document.body.textContent).toContain(review.title); expect(api.query).toHaveBeenCalledTimes(3);
  finish({ kind: "review", review: { ...review, title: "Updated review", head_revision: "new" } }); await settle();
  expect(document.body.textContent).toContain("Updated review"); expect(api.query).toHaveBeenCalledTimes(4);
});

it("uses one file header and shared display options across lazy file switches", async () => {
  api.query.mockImplementation((_provider, _host, input) => {
    if (input.operation === "get") return Promise.resolve({ kind: "review", review: { ...review, changed_files: 2 } });
    if (input.operation === "checks") return Promise.resolve({ kind: "checks", checks: [], next_cursor: null });
    if (input.operation === "files") return Promise.resolve({ kind: "files", files: ["first.rs", "second.rs"].map(path => ({ path, status: "modified", additions: 1, deletions: 1 })), next_cursor: null });
    if (input.operation === "diff") return Promise.resolve({ kind: "diff", path: input.path, patch: "@@ -1 +1 @@\n-old\n+new" });
  });
  component = mount(HostedReviewPanel, { target: document.body, props: { reference } }); await settle(); click("Changes"); await settle();
  document.querySelector<HTMLButtonElement>(".file-row")!.click(); await settle();
  expect(document.querySelectorAll('button[aria-label="Diff view options"]')).toHaveLength(1);
  expect(document.querySelector(".diff-file-header")).toBeNull();
  expect(document.querySelector(".diff-hunk-meta")).toBeNull();
  document.querySelector<HTMLButtonElement>('button[aria-label="Diff view options"]')!.click(); await settle();
  [...document.querySelectorAll<HTMLButtonElement>('[role="menuitemcheckbox"]')].find(b => b.textContent?.includes("Side by side"))!.click(); await settle();
  expect(document.querySelector(".diff-view--side")).not.toBeNull();
  document.querySelectorAll<HTMLButtonElement>(".file-row")[1].click(); await settle();
  expect(document.querySelectorAll(".diff-view")).toHaveLength(1);
  expect(document.querySelector(".diff-view--side")).not.toBeNull();
  expect(api.query.mock.calls.filter(call => call[2].operation === "diff")).toHaveLength(2);
});
