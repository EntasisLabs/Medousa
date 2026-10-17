import { afterEach, describe, expect, it, vi } from "vitest";
import { workspace } from "$lib/stores/workspace.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { layout } from "$lib/runtime/layout.svelte";
import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
vi.mock("$lib/stores/workspace.svelte", () => ({ workspace: { selectCard: vi.fn() } }));
vi.mock("$lib/stores/commandSpotlight.svelte", () => ({ commandSpotlight: { closeSpotlight: vi.fn() } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { setMobileTab: vi.fn(), navigateDesktop: vi.fn() } }));
import {
  openVaultNote,
  openWorkCard,
  setShellUseCasePortsForTests,
} from "./shellUseCases";

afterEach(() => {
  setShellUseCasePortsForTests({});
  setActiveWorkshopIdPort(null);
  vi.clearAllMocks();
});

describe("shell use cases", () => {
  it("opens fresh shared task details while preserving the current destination", async () => {
    await openWorkCard("card-1");
    expect(commandSpotlight.closeSpotlight).toHaveBeenCalledOnce();
    expect(workspace.selectCard).toHaveBeenCalledWith("card-1", { refresh: true });
    expect(layout.setMobileTab).not.toHaveBeenCalled(); expect(layout.navigateDesktop).not.toHaveBeenCalled();
  });
  it("drops an opening request when its workshop changes during lazy loading", async () => {
    setActiveWorkshopIdPort(() => "one"); const pending = openWorkCard("same-id");
    setActiveWorkshopIdPort(() => "two"); await pending;
    expect(workspace.selectCard).not.toHaveBeenCalled(); expect(commandSpotlight.closeSpotlight).not.toHaveBeenCalled();
  });
  it("openVaultNote uses injected ports instead of vault stores", async () => {
    const opened: string[] = [];
    setShellUseCasePortsForTests({
      openVaultNote: async (path) => {
        opened.push(path);
      },
    });
    await openVaultNote("notes/a.md");
    expect(opened).toEqual(["notes/a.md"]);
  });

  it("openWorkCard uses injected ports instead of workspace store", async () => {
    const opened: string[] = [];
    setShellUseCasePortsForTests({
      openWorkCard: async (id) => {
        opened.push(id);
      },
    });
    await openWorkCard("card-1");
    expect(opened).toEqual(["card-1"]);
  });
});
