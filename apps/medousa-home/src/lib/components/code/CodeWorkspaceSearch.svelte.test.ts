/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { invalidateCodeWorkshopContext } from "$lib/code/codeWorkspaceContext.svelte";
const api = vi.hoisted(() => ({ replace: vi.fn(), begin: vi.fn(), activate: vi.fn(), search: vi.fn() }));
vi.mock("$lib/forge", () => ({
  canStartHumanEditing: () => true, startHumanEditingSession: api.begin,
  humanizeForgeMessage: (message: string) => message,
  replaceUndertakingSource: api.replace, searchUndertakingSource: api.search,
}));
vi.mock("$lib/stores/undertakings.svelte", () => ({ undertakings: {
  active: { workId: "work", leaseId: null, leaseGeneration: null },
  detail: { id: "work", allowed_actions: {} }, setActiveFromItem: api.activate,
} }));
import CodeWorkspaceSearch from "./CodeWorkspaceSearch.svelte";

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
  vi.clearAllMocks();
  localStorage.clear();
});

it("does not apply a replacement when editing control arrives after a workshop switch", async () => {
  api.replace.mockResolvedValueOnce({ files: [{
    path: "app.ts", before: "old", after: "new", expected_digest: "digest", match_count: 1,
  }] });
  let resolve!: (value: unknown) => void;
  api.begin.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const target = document.createElement("div");
  document.body.append(target);
  component = mount(CodeWorkspaceSearch, { target, props: { workId: "work", workspaceScope: "workshop-a/work" } });
  flushSync();
  const input = target.querySelector<HTMLInputElement>('input[placeholder="Search in project…"]')!;
  input.value = "old";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  const button = (label: string) => Array.from(document.querySelectorAll("button"))
    .find((element) => element.textContent?.trim() === label)!;
  button("Replace").click();
  flushSync();
  button("Review replace…").click();
  await vi.waitFor(() => expect(button("Apply replace")).toBeTruthy());
  button("Apply replace").click();
  await vi.waitFor(() => expect(api.begin).toHaveBeenCalledOnce());
  invalidateCodeWorkshopContext();
  resolve({ item: { id: "work" }, lease: { lease_id: "old-lease", generation: 1 } });
  await Promise.resolve();
  flushSync();
  expect(api.activate).not.toHaveBeenCalled();
  expect(api.replace).toHaveBeenCalledOnce();
  expect(api.replace.mock.calls[0]![1].dryRun).toBe(true);
});


it("keeps basic search focused and discards a late result from the previous query", async () => {
  let resolve!: (value: unknown) => void;
  api.search.mockReturnValueOnce(new Promise((done) => { resolve = done; })).mockResolvedValueOnce({ hits: [{ path: "new.ts", line: 2, preview: "new code" }], truncated: false });
  component = mount(CodeWorkspaceSearch, { target: document.body, props: { workId: "work", workspaceScope: "scope" } }); flushSync();
  expect(document.querySelector('input[placeholder="Replace with…"]')).toBeNull();
  expect(document.querySelector('input[placeholder="files to include"]')).toBeNull();
  const input = document.querySelector<HTMLInputElement>('[aria-label="Search in project"]')!;
  const search = () => Array.from(document.querySelectorAll("button")).find((button) => button.textContent?.trim() === "Search")!;
  input.value = "old"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); search().click();
  await vi.waitFor(() => expect(api.search).toHaveBeenCalledOnce());
  input.value = "new"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); search().click();
  await vi.waitFor(() => expect(document.querySelector('[data-search-hit]')?.textContent).toContain("new code"));
  resolve({ hits: [{ path: "old.ts", line: 1, preview: "old code" }], truncated: false });
  await Promise.resolve(); flushSync();
  expect(document.body.textContent).not.toContain("old.ts");
  expect(document.querySelector("mark")?.textContent).toBe("new");
});

it("invalidates a replacement preview when its query changes while the preview is pending", async () => {
  let resolve!: (value: unknown) => void;
  api.replace.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  component = mount(CodeWorkspaceSearch, { target: document.body, props: { workId: "work", workspaceScope: "scope" } }); flushSync();
  const input = document.querySelector<HTMLInputElement>('[aria-label="Search in project"]')!;
  input.value = "old"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  const button = (label: string) => Array.from(document.querySelectorAll("button")).find((el) => el.textContent?.trim() === label)!;
  button("Replace").click(); flushSync(); button("Review replace…").click();
  await vi.waitFor(() => expect(api.replace).toHaveBeenCalledOnce());
  input.value = "new"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  resolve({ files: [{ path: "old.ts", before: "old", after: "replacement", expected_digest: "digest", match_count: 1 }] });
  await Promise.resolve(); flushSync();
  expect(document.querySelector('[aria-label="Review replace"]')).toBeNull();
});
