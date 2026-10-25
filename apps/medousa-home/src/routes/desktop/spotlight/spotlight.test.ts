/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const api = vi.hoisted(() => ({ invoke: vi.fn(), hide: vi.fn(), setFocus: vi.fn(async () => {}), stop: vi.fn(), health: null as null | ((health: { ok: boolean; message: string }) => void), focus: null as null | ((event: { payload: boolean }) => void), drag: null as null | ((event: { payload: { type: "enter" | "over" | "drop" | "leave" } }) => void), events: new Map<string, () => void>(), fail: null as null | ((cause: Error) => void) }));
const catalogs = vi.hoisted(() => ({ canvases: vi.fn(), projects: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: api.invoke }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ hide: api.hide, setFocus: api.setFocus, onDragDropEvent: async (callback: typeof api.drag) => { api.drag = callback; return () => { api.drag = null; }; }, onFocusChanged: async (callback: typeof api.focus) => { api.focus = callback; return () => { api.focus = null; }; } }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async (name: string, callback: () => void) => { api.events.set(name, callback); return () => { api.events.delete(name); }; } }));
vi.mock("$lib/desktop/connect", () => ({ connectDesktopWindow: (_scope: string, ready: () => void, onHealth: NonNullable<typeof api.health>) => { api.health = onHealth; queueMicrotask(ready); return api.stop; } }));
vi.mock("$lib/workshopConnection", () => ({ prepareForWorkshopSwitch: vi.fn() }));
vi.mock("$lib/create/creativeApi", () => ({ creativeApi: () => ({ list: catalogs.canvases }) }));
vi.mock("$lib/create/creativeBridge", () => ({ creativeBridgeApi: () => ({ projects: catalogs.projects }) }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: { notes: [1], labelByPathMap: new Map() } }));
vi.mock("$lib/stores/workshop.svelte", () => ({ workshop: { scripts: [1] } }));
vi.mock("$lib/terminal", () => ({ terminalSessions: async () => [] }));
import Launcher from "./+page.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
let component: ReturnType<typeof mount> | undefined;
async function settle(ms = 40) { await new Promise(resolve => setTimeout(resolve, ms)); flushSync(); }
beforeEach(() => {
  vi.clearAllMocks(); api.events.clear(); api.fail = null; api.health = null;
  catalogs.canvases.mockResolvedValue({ projects: [] }); catalogs.projects.mockResolvedValue([]);
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
it("keeps the launcher quiet and usable when optional catalogs fail", async () => {
  catalogs.canvases.mockRejectedValue(new Error("Create catalog offline"));
  catalogs.projects.mockRejectedValue(new Error("Code catalog offline"));
  component = mount(Launcher, { target: document.body }); await settle(80);
  expect(document.body.textContent).toContain("Save current file");
  expect(document.querySelector(".desktop-launcher-notice")).toBeNull();
  expect(document.body.textContent).not.toContain("Space");
});
afterEach(async () => { commandSpotlight.closeSpotlight(); if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });

async function handoff() {
  component = mount(Launcher, { target: document.body }); await settle(80);
  const action = [...document.querySelectorAll<HTMLButtonElement>(".command-spotlight-row")].find(button => button.textContent?.includes("Save current file"));
  expect(action).toBeDefined(); action!.click(); await settle();
  expect(api.fail).not.toBeNull();
  api.focus?.({ payload: false }); flushSync();
  expect(commandSpotlight.open).toBe(true);
  expect(api.hide).toHaveBeenCalled();
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

it("keeps Spotlight visible through an external press and drop, then handles ordinary blur", async () => {
  component = mount(Launcher, { target: document.body }); await settle(80);
  api.invoke.mockImplementation(async name => name === "desktop_spotlight_pointer" ? { buttonsDown: true, inside: false } : null);
  api.focus?.({ payload: false }); await settle();
  expect(commandSpotlight.open).toBe(true);
  expect(api.hide).not.toHaveBeenCalled();
  api.drag?.({ payload: { type: "enter" } });
  expect(api.setFocus).not.toHaveBeenCalled();
  api.drag?.({ payload: { type: "drop" } });
  expect(api.setFocus).toHaveBeenCalledOnce();
  api.focus?.({ payload: true }); await settle();
  expect(commandSpotlight.open).toBe(true);
  api.invoke.mockImplementation(async name => name === "desktop_spotlight_pointer" ? { buttonsDown: false, inside: false } : null);
  api.focus?.({ payload: false }); await settle();
  expect(commandSpotlight.open).toBe(false);
  expect(api.hide).toHaveBeenCalled();
});
it("lets the shortcut dismiss a held outside press without a later probe reopening it", async () => {
  component = mount(Launcher, { target: document.body }); await settle(80);
  api.invoke.mockImplementation(async name => name === "desktop_spotlight_pointer" ? { buttonsDown: true, inside: false } : null);
  api.focus?.({ payload: false }); await settle();
  api.events.get("desktop-spotlight-dismiss")?.(); await settle();
  expect(commandSpotlight.open).toBe(false);
  const count = api.invoke.mock.calls.length;
  await settle(65);
  expect(api.invoke.mock.calls.length).toBe(count);
});

it("keeps a fatal connection failure compact until the workshop reconnects", async () => {
  component = mount(Launcher, { target: document.body }); await settle(80);
  api.health!({ ok: false, message: "connection refused" }); flushSync();
  const notice = document.querySelector<HTMLDetailsElement>(".desktop-launcher-notice")!;
  expect(notice.open).toBe(false);
  expect(notice.querySelector("summary")?.textContent).toBe("Couldn’t connect to this workshop");
  expect(document.querySelector('[role="alert"]')).toBeNull();
  api.events.get("desktop-spotlight-open")?.(); await settle();
  expect(document.querySelector(".desktop-launcher-notice")).not.toBeNull();
  api.health!({ ok: true, message: "Connected" }); flushSync();
  expect(document.querySelector(".desktop-launcher-notice")).toBeNull();
});
