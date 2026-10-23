import { expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ load: vi.fn(), update: vi.fn() }));
vi.mock("$lib/platform", async original => ({ ...await original<typeof import("$lib/platform")>(), isTauri: () => true }));
vi.mock("$lib/workshops", async original => ({ ...await original<typeof import("$lib/workshops")>(), loadWorkshopRegistry: api.load, updateWorkshopClientState: api.update }));
vi.mock("$lib/daemon", async original => ({ ...await original<typeof import("$lib/daemon")>(), getDaemonUrl: async () => "http://localhost:7419" }));
import { WorkshopsStore } from "$lib/stores/workshops.svelte";
import { defaultWorkshopRegistry } from "$lib/types/workshopRegistry";

it("keeps a resource window pinned when metadata receipts carry the main window’s new selection", async () => {
  const registry = defaultWorkshopRegistry();
  registry.workshops.push({ ...registry.workshops[0], id: "retained", label: "Studio" });
  registry.activeWorkshopId = "personal";
  api.load.mockResolvedValue(registry);
  api.update.mockResolvedValue(registry);
  const store = new WorkshopsStore();
  await store.load({ workshopId: "retained", restoreShell: false });
  expect(store.activeWorkshopId).toBe("retained");
  await store.saveActiveSession("conversation");
  expect(api.update).toHaveBeenCalledWith("retained", { lastSessionId: "conversation" });
  expect(store.activeWorkshopId).toBe("retained");
  await store.load({ restoreShell: false });
  expect(store.activeWorkshopId).toBe("retained");
});

it("ignores an old metadata receipt after the main window has switched workshops", async () => {
  const registry = defaultWorkshopRegistry();
  let finish!: (value: typeof registry) => void;
  api.update.mockImplementationOnce(() => new Promise(resolve => finish = resolve));
  const store = new WorkshopsStore();
  const saving = store.saveActiveSession("old-session");
  store.registry = { ...registry, activeWorkshopId: "new-workshop" };
  finish(registry); await saving;
  expect(store.activeWorkshopId).toBe("new-workshop");
});
