import { beforeEach, expect, it, vi } from "vitest";

const ports = vi.hoisted(() => ({
  layout: { isMobile: false, openWeb: vi.fn(), navigateDesktop: vi.fn() },
  browser: { linkSession: vi.fn(), linkWorkCard: vi.fn(), ensureTabGroup: vi.fn(), syncFromNative: vi.fn() },
  governedBrowser: { chooseDevice: vi.fn() },
  humanBrowser: { openTab: vi.fn(), navigate: vi.fn() },
}));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: ports.layout }));
vi.mock("$lib/stores/browser.svelte", () => ({ browser: ports.browser }));
vi.mock("$lib/stores/governedBrowser.svelte", () => ({ governedBrowser: ports.governedBrowser }));
vi.mock("$lib/stores/humanBrowser.svelte", () => ({ humanBrowser: ports.humanBrowser }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { sessionId: null } }));
vi.mock("$lib/utils/agentBrowserCoord", () => ({ markAgentNavigation: vi.fn() }));
vi.mock("$lib/window", () => ({ isTauri: () => true }));
import { openInBrowser } from "./openInBrowser";

beforeEach(() => { vi.clearAllMocks(); ports.layout.isMobile = false; });

it.each([false, true])("opens review links as new browser tabs without navigating the existing page (mobile=%s)", async (mobile) => {
  ports.layout.isMobile = mobile;
  await openInBrowser("https://example.com/review", { openedBy: "user", newTab: true });
  expect(ports.governedBrowser.chooseDevice).toHaveBeenCalledOnce();
  expect(ports.humanBrowser.openTab).toHaveBeenCalledExactlyOnceWith("https://example.com/review");
  expect(ports.humanBrowser.navigate).not.toHaveBeenCalled();
  expect(ports.browser.syncFromNative).toHaveBeenCalledWith("https://example.com/review");
  if (mobile) expect(ports.layout.openWeb).toHaveBeenCalledOnce();
  else expect(ports.layout.navigateDesktop).toHaveBeenCalledWith("web");
});

it("preserves the existing navigation behavior for callers that do not request a new tab", async () => {
  await openInBrowser("https://example.com/page");
  expect(ports.humanBrowser.navigate).toHaveBeenCalledExactlyOnceWith("https://example.com/page");
  expect(ports.humanBrowser.openTab).not.toHaveBeenCalled();
});
