/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import ChatConnectionStatus from "./ChatConnectionStatus.svelte";
import { connection } from "$lib/stores/connection.svelte";
import type { DaemonHealth } from "$lib/daemon";

const api = vi.hoisted(() => ({ refresh: vi.fn(), diagnose: vi.fn(), start: vi.fn(), settings: vi.fn(), more: vi.fn(), local: false }));
vi.mock("$lib/stores/workshops.svelte", () => ({ workshops: {
  get activeWorkshop() { return { kind: api.local ? "local" : "portal" }; },
  refreshing: false, switching: false, refreshConnection: api.refresh,
} }));
vi.mock("$lib/platform", () => ({ isTauriMobilePlatform: () => !api.local }));
vi.mock("$lib/window", () => ({ isTauri: () => true }));
vi.mock("$lib/stores/settingsNav.svelte", () => ({ settingsNav: { setActiveSection: api.settings } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { openMore: api.more } }));
vi.mock("$lib/utils/engineDiagnosticsApi", () => ({ diagnoseEngine: api.diagnose, clearEngineStaleLock: vi.fn(), openEngineLog: vi.fn() }));
vi.mock("$lib/utils/providersApi", () => ({ startEngine: api.start, restartEngine: vi.fn(), waitForEngine: vi.fn() }));
vi.mock("$lib/workshopConnection", () => ({ reconnectWorkshop: vi.fn() }));

let component: ReturnType<typeof mount> | undefined;
const failed = { ok: false, message: "iroh request failed for GET /v1/health: timeout" } as DaemonHealth;
async function settle() { flushSync(); await tick(); flushSync(); }
function button(label: string) { return [...document.querySelectorAll("button")].find(item => item.textContent?.trim() === label)!; }
beforeEach(() => {
  vi.clearAllMocks(); api.local = false;
  api.diagnose.mockResolvedValue({ title: "Medousa isn't running", message: "Start Medousa", issue: "not_running" });
  connection.setHealth({ ok: true } as DaemonHealth); connection.setRecovering(false);
});
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });

it("keeps recovery non-modal and quiet through repeated background failures", async () => {
  component = mount(ChatConnectionStatus, { target: document.body }); await settle();
  const status = document.querySelector(".chat-connection-status");
  for (let attempt = 0; attempt < 5; attempt++) {
    connection.setHealth(failed); await settle();
    expect(document.querySelector(".chat-connection-status")).toBe(status);
    expect(document.querySelector('[role="dialog"], [role="alertdialog"], [aria-modal="true"]')).toBeNull();
    expect(document.body.textContent).toContain("Reconnecting…");
    expect(document.body.textContent).not.toContain("iroh request failed");
    expect(document.body.textContent).not.toContain("Medousa isn't connected");
    connection.setHealth({ ok: true } as DaemonHealth); await settle();
    expect(document.querySelector(".chat-connection-status")).toBe(status);
    expect(document.body.textContent).not.toContain("Reconnecting…");
  }
  expect(api.refresh).not.toHaveBeenCalled();
  expect(api.diagnose).not.toHaveBeenCalled();
  expect(api.start).not.toHaveBeenCalled();
  expect(api.more).not.toHaveBeenCalled();
});

it("provides explicit retry and connection settings without reopening pairing", async () => {
  connection.setHealth(failed);
  component = mount(ChatConnectionStatus, { target: document.body }); await settle();
  button("Retry").click(); await settle();
  expect(api.refresh).toHaveBeenCalledOnce();
  connection.setRecovering(true); await settle();
  expect(button("Retry").disabled).toBe(true);
  button("Connection settings").click();
  expect(api.settings).toHaveBeenCalledWith("basement");
  expect(api.more).toHaveBeenCalledWith("settings");
});

it("only diagnoses a local engine when recovery details are explicitly opened", async () => {
  api.local = true; connection.setHealth(failed);
  component = mount(ChatConnectionStatus, { target: document.body }); await settle();
  expect(api.diagnose).not.toHaveBeenCalled();
  expect(document.querySelector('[aria-label="Connection recovery"]')).toBeNull();
  button("Details").click(); await settle();
  expect(api.diagnose).toHaveBeenCalledOnce();
  expect(document.querySelector('[aria-label="Connection recovery"]')).not.toBeNull();
  expect(api.start).not.toHaveBeenCalled();
  connection.setHealth({ ok: true } as DaemonHealth); await settle();
  connection.setHealth(failed); await settle();
  expect(document.querySelector('[aria-label="Connection recovery"]')).toBeNull();
  expect(api.diagnose).toHaveBeenCalledOnce();
});
