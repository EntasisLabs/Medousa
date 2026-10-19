/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const api = vi.hoisted(() => ({ query: vi.fn() }));
vi.mock("$lib/code/hostedReviews", () => ({ queryHostedReviews: api.query }));
import HostedReviewPanel from "./HostedReviewPanel.svelte";
const reference = { provider_id: "example", host: "git.example.test", repository: "group/repo", native_id: "opaque" };
const review = { reference, title: "A hosted review", body: "", state: "open", author: "author", head_branch: "feature", base_branch: "main", head_revision: "abc", updated_at: "2026-10-18", changed_files: 1, additions: 1, deletions: 0, url: "https://git.example.test/review/opaque" };
let component: ReturnType<typeof mount> | null = null;
async function settle() { for (let i = 0; i < 8; i++) { await Promise.resolve(); flushSync(); } }
function click(label: string) { const button = [...document.querySelectorAll("nav button")].find((b) => b.textContent?.startsWith(label)) as HTMLButtonElement; button.click(); }
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); sessionStorage.clear(); vi.clearAllMocks(); });

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
