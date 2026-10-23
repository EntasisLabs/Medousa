/** @vitest-environment happy-dom */
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync, tick } from "svelte";
import CreateRail from "./CreateRail.svelte";
import { createBrowser } from "$lib/create/createBrowser.svelte";
import { loadCreativeRailPreferences } from "$lib/create/creativeRailPreferences";
import { acquireCreativeProject } from "$lib/create/creativeWorkspace.svelte";
import type { CreativeDocument } from "$lib/types/generated/daemon_api";
const { request, tabs } = vi.hoisted(() => ({ request: vi.fn(), tabs: { tabs: [] as unknown[], activeTab: null, patchTitle: vi.fn(), close: vi.fn() } }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: request }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: tabs }));
let components: ReturnType<typeof mount>[] = [];
let scope: string, docs: Map<string, CreativeDocument>;
let onopen = vi.fn(), oncreate = vi.fn();
const settle = async () => { await tick(); await new Promise(r => setTimeout(r, 5)); flushSync(); await tick(); };
function button(name: string, root: ParentNode = document) {
  const item = [...root.querySelectorAll<HTMLButtonElement>("button")].find(b => b.getAttribute("aria-label") === name || b.textContent?.trim() === name);
  expect(item, `button ${name}`).toBeTruthy(); return item!;
}
function render() {
  const target = document.createElement("div"); document.body.append(target);
  components.push(mount(CreateRail, { target, props: { browser: createBrowser(scope), scope, onopen, oncreate } }));
  flushSync(); return target;
}
function rows(root: ParentNode) { return [...root.querySelectorAll<HTMLButtonElement>("[data-project-open]")].map(b => b.textContent?.trim()); }
beforeEach(() => {
  localStorage.clear(); scope = crypto.randomUUID(); onopen = vi.fn(); oncreate = vi.fn();
  tabs.tabs = []; tabs.patchTitle.mockClear(); tabs.close.mockClear();
  docs = new Map(["Zebra", "Alpha", "Pinned"].map((title, i) => [title, {
    id: title, title, pinned: title === "Pinned", revision: 0, created_at: "now", updated_at: `2026-10-${22 - i}T00:00:00Z`, assets: {},
    scene: { schema: "medousa-create", version: 3, background: "transparent", nodes: [] },
  }]));
  for (const doc of docs.values()) createBrowser(scope).acknowledge(doc);
  request.mockReset();
  request.mockImplementation(async (op, params, body) => {
    const doc = docs.get(params.id)!;
    if (op.endsWith("jobs.get")) return { jobs: [] };
    if (op === "create.projects.by_id.delete.post") { expect(body.expected_revision).toBe(doc.revision); docs.delete(doc.id); return { deleted: true }; }
    if (op.endsWith("transactions.post")) {
      expect(body.expected_revision).toBe(doc.revision);
      for (const edit of body.operations) { if (edit.op === "rename") doc.title = edit.title; if (edit.op === "pin") doc.pinned = edit.pinned; }
      doc.revision++;
    }
    return { document: structuredClone(doc), applied_revision: doc.revision, duplicate: false, can_undo: doc.revision > 0, can_redo: false };
  });
});
afterEach(async () => { for (const c of components) await unmount(c); components = []; document.body.replaceChildren(); });
it("keeps search inline and finds projects inside remembered collapsed sections", async () => {
  const root = render();
  expect(root.querySelector('input[type="search"]')).toBeNull();
  button("Recent", root).click(); await settle();
  expect(rows(root)).toEqual(["Pinned"]);
  button("Search projects", root).click(); await settle();
  const input = root.querySelector<HTMLInputElement>('input[type="search"]')!;
  expect(document.activeElement).toBe(input);
  input.value = "alpha"; input.dispatchEvent(new Event("input", { bubbles: true })); await settle();
  expect(rows(root)).toEqual(["Alpha"]);
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await settle();
  expect(rows(root)).toEqual(["Pinned"]);
  expect(loadCreativeRailPreferences(scope).recentOpen).toBe(false);
});
it("uses the existing starter choices and remembers sorting and scroll position across remounts", async () => {
  const root = render();
  button("New project", root).click(); await settle(); button("Image composition").click(); await settle();
  expect(oncreate).toHaveBeenCalledWith("composition");
  button("Project list options", root).click(); await settle(); button("Name").click(); await settle();
  expect(rows(root)).toEqual(["Pinned", "Alpha", "Zebra"]);
  const list = root.querySelector<HTMLDivElement>(".create-rail-list")!;
  list.scrollTop = 85; list.dispatchEvent(new Event("scroll")); await settle();
  await unmount(components.pop()!); root.remove(); const next = render(); await settle();
  expect(next.querySelector(".create-rail-list")!.scrollTop).toBe(85);
  expect(loadCreativeRailPreferences(scope)).toMatchObject({ sort: "name", scrollTop: 85 });
});
it("renames through the shared session and updates matching shell tabs only", async () => {
  tabs.tabs = [{ id: "one", kind: "create", documentId: "Alpha", workshopId: scope }, { id: "other", kind: "create", documentId: "Alpha", workshopId: "elsewhere" }];
  const lease = acquireCreativeProject(scope, "Alpha"); await lease.session.load();
  const root = render(), row = root.querySelector<HTMLButtonElement>('[data-project-id="Alpha"] [data-project-open]')!;
  row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })); await settle();
  button("RenameF2").click(); await settle();
  const input = root.querySelector<HTMLInputElement>('[aria-label="Project name"]')!;
  input.value = "New title"; input.dispatchEvent(new Event("input", { bubbles: true })); await settle();
  button("Save project name", root).click(); await settle();
  expect(lease.session.document?.title).toBe("New title");
  expect(rows(root)).toContain("New title");
  expect(tabs.patchTitle).toHaveBeenCalledExactlyOnceWith("one", "New title");
  lease.release();
});
it("pins projects, requires delete confirmation, and closes only deleted project tabs", async () => {
  tabs.tabs = [{ id: "one", kind: "create", documentId: "Alpha", workshopId: scope }, { id: "other", kind: "create", documentId: "Alpha", workshopId: "elsewhere" }];
  const root = render();
  button("Actions for Alpha", root).click(); await settle(); button("Pin project").click(); await settle();
  expect(createBrowser(scope).projects.find(p => p.id === "Alpha")?.pinned).toBe(true);
  button("Actions for Alpha", root).click(); await settle(); button("Delete project…").click(); await settle();
  expect(docs.has("Alpha")).toBe(true); expect(document.activeElement).toBe(button("Cancel", root));
  button("Cancel", root).dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); await settle();
  expect(docs.has("Alpha")).toBe(true);
  const row = root.querySelector<HTMLButtonElement>('[data-project-id="Alpha"] [data-project-open]')!;
  row.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true, cancelable: true })); await settle();
  button("Delete project", root).click(); await settle();
  expect(docs.has("Alpha")).toBe(false); expect(rows(root)).not.toContain("Alpha");
  expect(tabs.close).toHaveBeenCalledExactlyOnceWith("one");
});
it("navigates native buttons and cancels inline rename without opening or mutating a project", async () => {
  const root = render(), first = root.querySelector<HTMLButtonElement>("[data-project-open]")!;
  first.focus(); first.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true, cancelable: true }));
  expect(document.activeElement?.textContent?.trim()).toBe("Alpha");
  document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "F2", bubbles: true, cancelable: true })); await settle();
  const input = root.querySelector<HTMLInputElement>('[aria-label="Project name"]')!;
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); await settle();
  expect(root.querySelector('[aria-label="Project name"]')).toBeNull(); expect(onopen).not.toHaveBeenCalled();
  expect(request.mock.calls.some(c => c[0].endsWith("transactions.post"))).toBe(false);
});
