/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { CreativeDocument } from "$lib/types/generated/daemon_api";
import type { CreativeProjectLease } from "$lib/create/creativeWorkspace.svelte";
const f = vi.hoisted(() => ({ request: vi.fn(), scope: "one", chat: vi.fn(), capture: vi.fn(), dirty: false }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: f.request }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => f.scope }));
vi.mock("$lib/utils/browserPopoverOverlay", () => ({ pushBrowserPopoverOverlay: async () => {}, popBrowserPopoverOverlay: async () => {} }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openChat: f.chat } }));
vi.mock("$lib/stores/codeWorkspace.svelte", () => ({ codeWorkspace: { tabsFor: () => ["file"], isDirty: () => f.dirty } }));
import CreateCodeBridge from "./CreateCodeBridge.svelte";
let component: ReturnType<typeof mount> | null = null;
let doc: CreativeDocument;
const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].filter(b => b.textContent?.trim() === text).at(-1)!;
async function settle() { await new Promise(r => setTimeout(r, 0)); flushSync(); }
async function show() {
  const lease = { api: { scope: "one" }, session: { document: doc, dirty: false, busy: false, enqueue: vi.fn(), save: vi.fn() } } as unknown as CreativeProjectLease;
  component = mount(CreateCodeBridge, { target: document.body, props: { lease, document: doc, frame: doc.scene.nodes[0], disabled: false, capture: f.capture } });
  flushSync(); button("Update Code").click(); await settle();
}
beforeEach(() => {
  vi.clearAllMocks(); f.scope = "one"; f.dirty = false;
  doc = { id: "design", title: "Shell", revision: 7, scene: { nodes: [{ id: "frame", visible: true, locked: false, content: { kind: "frame", name: "Shell", background: "#fff" }, transform: { x: 0, y: 0, width: 900, height: 600, rotation: 0 }, design_source: { work_id: "repo", session_id: "thread", url: "", viewport_width: 900, viewport_height: 600, captured_at_ms: 1 } }] } } as CreativeDocument;
  f.capture.mockResolvedValue("visual");
  f.request.mockImplementation(async (op: string) => {
    if (op === "forge.items.get") return [{ id: "repo", title: "Medousa" }];
    if (op === "forge.items.by_work_id.get") return { id: "repo", workspace_present: true };
    if (op === "sessions.by_session_id.code_binding.get") return { work_id: "repo" };
    if (op === "create.projects.by_id.code_context.post") return { context: { revision: 7, nodes: doc.scene.nodes }, media_refs: [{ id: "snapshot", kind: "image" }] };
    return {};
  });
  vi.spyOn(HTMLDialogElement.prototype, "showModal").mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  vi.spyOn(HTMLDialogElement.prototype, "close").mockImplementation(function (this: HTMLDialogElement) { this.open = false; });
});
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); vi.restoreAllMocks(); });
it("uses the linked conversation, exact saved visual and existing Code runtime only on explicit submit", async () => {
  await show(); expect(f.request.mock.calls.some(c => c[0] === "turns.post")).toBe(false);
  button("Update Code").click(); await settle();
  expect(f.capture).toHaveBeenCalledWith(7, ["frame"]);
  expect(f.request).toHaveBeenCalledWith("create.projects.by_id.code_context.post", { id: "design" }, { frame_id: "frame", expected_revision: 7, visual_asset_id: "visual", session_id: "thread" }, null, undefined, "one");
  expect(f.request.mock.calls.find(c => c[0] === "turns.post")?.[2]).toMatchObject({ session_id: "thread", agent_mode: "coder", code_context: { work_id: "repo" }, media_refs: [{ id: "snapshot", kind: "image" }] });
  expect(f.chat).toHaveBeenCalledWith("thread", { activate: true });
});
it("blocks dirty Code files before capturing or starting a turn", async () => {
  await show(); f.dirty = true; button("Update Code").click(); await settle();
  expect(document.body.textContent).toContain("Save your open Code files"); expect(f.capture).not.toHaveBeenCalled();
  expect(f.request.mock.calls.some(c => c[0] === "turns.post")).toBe(false);
});
it("does not send a turn when the workshop changes while the saved visual is captured", async () => {
  await show(); f.capture.mockImplementation(async () => { f.scope = "two"; return "visual"; });
  button("Update Code").click(); await settle();
  expect(f.request.mock.calls.some(c => c[0] === "turns.post")).toBe(false); expect(f.chat).not.toHaveBeenCalled();
});
it("recovers uncertain admission through the conversation rather than offering an automatic resend", async () => {
  await show(); const prior = f.request.getMockImplementation()!;
  f.request.mockImplementation((op, ...args) => op === "turns.post" ? Promise.reject(new Error("Timed out")) : prior(op, ...args));
  button("Update Code").click(); await settle();
  expect(document.body.textContent).toContain("check whether the request started");
  expect(document.querySelector('button[type="submit"]')).toBeNull();
  button("Check conversation").click(); await settle(); expect(f.chat).toHaveBeenCalledWith("thread", { activate: true });
  expect(f.request.mock.calls.filter(c => c[0] === "turns.post")).toHaveLength(1);
});
