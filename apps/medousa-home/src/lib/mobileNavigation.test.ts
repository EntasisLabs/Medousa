import { describe, expect, it, vi } from "vitest";
vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: {} }));
vi.mock("$lib/stores/environment.svelte", () => ({ environment: {} }));
vi.mock("$lib/stores/workspace.svelte", () => ({ workspace: {} }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { sessionDrawerOpen: true, setSessionDrawerOpen: vi.fn() } }));
import { layout } from "$lib/runtime/layout.svelte";
import { registerMobileBackHandler, tryMobileBackNavigation } from "./mobileNavigation";

describe("modal back navigation", () => {
  it("lets the Bot editor consume Back before closing the session drawer", () => {
    const back = vi.fn(() => true);
    const unregister = registerMobileBackHandler(back, "modal");
    expect(tryMobileBackNavigation()).toBe(true);
    expect(back).toHaveBeenCalledOnce();
    expect(layout.setSessionDrawerOpen).not.toHaveBeenCalled();
    unregister();
    expect(tryMobileBackNavigation()).toBe(true);
    expect(layout.setSessionDrawerOpen).toHaveBeenCalledWith(false);
  });
});
