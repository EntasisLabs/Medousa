/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount, type Component } from "svelte";
import ContextBrowse from "./ContextBrowse.svelte";
import MapSidePanel from "./MapSidePanel.svelte";
import MapPanel from "./MapPanel.svelte";
import { contextShell } from "$lib/context/contextShell.svelte";
import { contextNotes } from "$lib/context/contextNotes.svelte";
import { contextThreads } from "$lib/context/contextThreads.svelte";
const api = vi.hoisted(() => ({ note: vi.fn(), moment: vi.fn(), notes: vi.fn(), list: vi.fn(), openNote: vi.fn(), refreshChats: vi.fn() }));
vi.mock("$lib/daemon", () => ({ getVaultNote: api.note, getLocusNode: api.moment, listVaultNotes: api.notes, listLocusNodes: api.list }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { sessions: [], workshopScopeId: "test", switchSession: vi.fn(), refreshSessions: api.refreshChats } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openChat: vi.fn() } }));
vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({ lmeWorkspace: { openNote: api.openNote } }));
const node = { sync_key: "r1", session_id: "source-session", tier: "raw", timestamp: "2026-10-17T12:00:00Z", context_summary: "checkpoint: Terminal checkpoint: Actual saved work", psi: 0, rho: 0, kappa: 0 };
const second = { ...node, sync_key: "r2", context_summary: "handoff: Handoff: A reviewable candidate", timestamp: "2026-10-16T12:00:00Z" };
const components: ReturnType<typeof mount>[] = [];
function render<Props extends Record<string, any>>(component: Component<Props>, props: Props, target = document.body) { const result = mount(component, { target, props }); components.push(result); flushSync(); return result; }
async function settle() { for (let i = 0; i < 15; i++) { await Promise.resolve(); flushSync(); } }
beforeEach(() => {
  api.list.mockResolvedValue({ nodes: [node, second] });
  api.notes.mockResolvedValue({ notes: [{ path: "linked.md", title: "Related research", modified_at_utc: node.timestamp, tags: ["chat:source-s"] }, { path: "unrelated.md", title: "Unrelated document", modified_at_utc: node.timestamp }] });
  api.moment.mockImplementation(async (key: string) => ({ node: key === "r2" ? second : node, raw: "Original persisted record" }));
});
afterEach(async () => {
  for (const component of components.splice(0)) await unmount(component);
  document.body.replaceChildren(); contextShell.selectMapNode(null); contextShell.search = ""; contextShell.lens = "browse";
  contextThreads.resetForWorkshopSwitch(); contextNotes.resetForWorkshopSwitch(); vi.resetAllMocks();
});

it("uses the rail as the only memory list and opens related notes without a second note reader", async () => {
  const rail = document.createElement("aside"), main = document.createElement("main"); document.body.append(rail, main);
  render(MapSidePanel, {}, rail); render(MapPanel, { visible: true }, main); await settle();
  expect(rail.querySelector('[aria-label="Saved memory list"]')).not.toBeNull();
  expect(main.querySelector('[aria-label="Saved memory list"]')).toBeNull();
  expect(document.querySelectorAll('[aria-label="Search saved memory"]')).toHaveLength(1);
  expect(rail.textContent).not.toContain("Related research"); expect(rail.textContent).not.toContain("Unrelated document");
  expect(main.querySelector("h2")?.textContent).toBe("Actual saved work");
  const related = main.querySelector('[aria-label="Related notes"]')!;
  related.querySelector("button")!.click(); flushSync(); await settle();
  expect(api.openNote).toHaveBeenCalledWith("linked.md"); expect(api.note).not.toHaveBeenCalled();
  rail.querySelector<HTMLButtonElement>('[data-memory-id="thread:r2"]')!.click(); flushSync(); await settle();
  expect(main.querySelector("h2")?.textContent).toBe("A reviewable candidate");
  const buttons = [...main.querySelectorAll<HTMLButtonElement>('[aria-label="Context lens"] button')];
  buttons.find((button) => button.textContent === "Map")!.click(); flushSync(); await settle();
  expect(contextShell.selectedMapNodeId).toBe("thread:r2");
  expect(rail.querySelector('[data-memory-id="thread:r2"]')?.getAttribute("aria-pressed")).toBe("true");
  buttons.find((button) => button.textContent === "Browse")!.click(); flushSync(); await settle();
  expect(main.querySelector("h2")?.textContent).toBe("A reviewable candidate");
  expect(rail.querySelector('[aria-label="Saved memory list"]')).not.toBeNull();
});

it("searches the memory rail without replacing the selected reader content", async () => {
  render(MapSidePanel, {}); await settle(); contextShell.selectMapNode("thread:r1");
  const field = document.querySelector<HTMLInputElement>('[aria-label="Search saved memory"]')!;
  field.value = "candidate"; field.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); await settle();
  expect(document.querySelectorAll("button[data-memory-id]")).toHaveLength(1);
  expect(contextShell.selectedMapNodeId).toBe("thread:r1");
});

it("lets mobile switch between its memory list and reader without duplicating columns", async () => {
  render(MapPanel, { visible: true, mobile: true }); await settle();
  expect(document.querySelector('[aria-label="Saved memory list"]')).not.toBeNull();
  document.querySelector<HTMLButtonElement>('[data-memory-id="thread:r2"]')!.click(); flushSync(); await settle();
  expect(document.querySelector('[aria-label="Saved memory list"]')).toBeNull();
  expect(document.querySelector("h2")?.textContent).toBe("A reviewable candidate");
  [...document.querySelectorAll("button")].find((button) => button.textContent?.trim() === "Saved memory")!.click(); flushSync(); await settle();
  expect(document.querySelector('[aria-label="Saved memory list"]')).not.toBeNull();
  expect(contextShell.selectedMapNodeId).toBe("thread:r2");
});

it("shows a failed moment read without automatically retrying in a reactive loop", async () => {
  api.moment.mockRejectedValue(new Error("Source unavailable"));
  contextShell.selectMapNode("thread:missing"); render(ContextBrowse, { sessionLabels: {} }); await settle();
  expect(api.moment).toHaveBeenCalledOnce(); expect(document.body.textContent).toContain("Source unavailable");
  [...document.querySelectorAll("button")].find((button) => button.textContent === "Retry saved moment")!.click(); flushSync(); await settle();
  expect(api.moment).toHaveBeenCalledTimes(2);
});
