/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const api = vi.hoisted(() => ({ invoke: vi.fn(), hide: vi.fn(), stop: vi.fn(), focus: null as null | ((event: { payload: boolean }) => void), events: new Map<string, () => void>(), fail: null as null | ((cause: Error) => void) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: api.invoke }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ hide: api.hide, onFocusChanged: async (callback: typeof api.focus) => { api.focus = callback; return () => { api.focus = null; }; } }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async (name: string, callback: () => void) => { api.events.set(name, callback); return () => { api.events.delete(name); }; } }));
vi.mock("$lib/desktop/connect", () => ({ connectDesktopWindow: (_scope: string, ready: () => void) => { queueMicrotask(ready); return api.stop; } }));
vi.mock("$lib/workshopConnection", () => ({ prepareForWorkshopSwitch: vi.fn() }));
vi.mock("$lib/create/creativeApi", () => ({ creativeApi: () => ({ list: async () => ({ projects: [] }) }) }));
vi.mock("$lib/create/creativeBridge", () => ({ creativeBridgeApi: () => ({ projects: async () => [] }) }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: { notes: [1], labelByPathMap: new Map() } }));
vi.mock("$lib/stores/workshop.svelte", () => ({ workshop: { scripts: [1] } }));
vi.mock("$lib/terminal", () => ({ terminalSessions: async () => [] }));
import Launcher from "./+page.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
let component: ReturnType<typeof mount> | undefined;
async function settle(ms = 40) { await new Promise(resolve => setTimeout(resolve, ms)); flushSync(); }
beforeEach(() => {
  vi.clearAllMocks(); api.events.clear(); api.fail = null;
  api.invoke.mockImplementation(async (name, args) => {
    if (name === "desktop_spotlight_context") return "personal";
    if (name === "desktop_spotlight_request" && args.request.kind === "collect") return {
      groups: [{ id: "actions", label: "Actions", commands: [{ id: "save-file", token: "save", section: "do", label: "Save current file" }] }],
      scopes: [{ id: "home", label: "Home" }], layout: null,
    };
    if (name === "desktop_spotlight_request" && args.request.kind === "execute") return new Promise((_resolve, reject) => { api.fail = reject; });
    return null;
  });
});
afterEach(async () => { commandSpotlight.closeSpotlight(); if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });

async function handoff() {
  component = mount(Launcher, { target: document.body }); await settle(80);
  const action = [...document.querySelectorAll<HTMLButtonElement>(".command-spotlight-row")].find(button => button.textContent?.includes("Save current file"));
  expect(action).toBeDefined(); action!.click(); await settle();
  expect(api.fail).not.toBeNull();
  api.focus?.({ payload: false }); flushSync();
  expect(commandSpotlight.open).toBe(true);
}
it("keeps a shell handoff pending across blur and restores an actionable failure", async () => {
  await handoff();
  api.fail!(new Error("Could not save this file")); await settle();
  expect(api.invoke).toHaveBeenCalledWith("desktop_spotlight_restore", { workshopId: "personal" });
  expect(commandSpotlight.open).toBe(true);
  expect(document.body.textContent).toContain("Could not save this file");
});
it("does not reopen the launcher when the operator dismisses a pending handoff", async () => {
  await handoff();
  api.events.get("desktop-spotlight-dismiss")?.(); flushSync();
  expect(commandSpotlight.open).toBe(false);
  api.fail!(new Error("Late failure")); await settle();
  expect(api.invoke.mock.calls.some(([name]) => name === "desktop_spotlight_restore")).toBe(false);
  expect(commandSpotlight.open).toBe(false);
});
