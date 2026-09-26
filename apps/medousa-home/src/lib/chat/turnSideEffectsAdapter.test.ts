import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

const mocks = vi.hoisted(() => ({
  fetch: vi.fn(),
  complete: vi.fn(),
  open: vi.fn(),
  snapshot: vi.fn(),
  control: vi.fn(),
}));
vi.mock("$lib/daemon", () => ({
  fetchBrowserSession: mocks.fetch,
  completeBrowserSession: mocks.complete,
}));
vi.mock("$lib/utils/openInBrowser", () => ({ openInBrowser: mocks.open }));
vi.mock("$lib/humanBrowser", () => ({ humanBrowserSnapshotSearch: mocks.snapshot }));
vi.mock("$lib/stores/humanBrowser.svelte", () => ({ humanBrowser: { loading: false } }));
vi.mock("$lib/stores/governedBrowser.svelte", () => ({ governedBrowser: { chooseDevice: vi.fn() } }));
vi.mock("$lib/stores/browser.svelte", () => ({ browser: { setControl: mocks.control } }));

import { handleBrowserChallenge } from "./turnSideEffectsAdapter";

describe("client browser search", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.fetch.mockResolvedValue({ query: "Medousa", max_results: 8, world_driver_id: "ios" });
  });

  function start(id: string) {
    const host = {
      sessionId: "chat",
      browserChallenge: null,
      messageIdForTurn: () => "message",
      workCardIdForTurn: () => null,
    } as unknown as ChatStoreHost;
    handleBrowserChallenge(host, {
      turn_id: "turn",
      message: "client_search",
      browser_session_id: id,
      browser_challenge_url: "https://html.duckduckgo.com/html/?q=Medousa",
    } as InteractiveTurnStreamEvent);
    return host;
  }

  it("opens the browser and completes a normal search automatically", async () => {
    const response = { results: [{ title: "Medousa", url: "https://example.com" }] };
    mocks.snapshot
      .mockResolvedValueOnce({ results: [], challenge: "empty_results" })
      .mockResolvedValue(response);
    const host = start("normal");
    await vi.waitFor(() => expect(mocks.complete).toHaveBeenCalledWith("normal", {
      worldDriverId: "ios", searchResponse: response,
    }), { timeout: 2000 });
    expect(mocks.open).toHaveBeenCalledOnce();
    expect(host.browserChallenge).toBeNull();
  });

  it("preserves a CAPTCHA page for the user instead of completing or reopening it", async () => {
    mocks.snapshot.mockResolvedValue({ results: [], challenge: "captcha" });
    const host = start("captcha");
    await vi.waitFor(() => expect(host.browserChallenge?.sessionId).toBe("captcha"), { timeout: 2000 });
    expect(mocks.control).toHaveBeenCalledWith("awaiting_operator");
    expect(mocks.open).toHaveBeenCalledOnce();
    expect(mocks.complete).not.toHaveBeenCalled();
  });

  it("reports browser failures rather than leaving the engine waiting", async () => {
    mocks.open.mockRejectedValue(new Error("Browser unavailable"));
    start("failure");
    await vi.waitFor(() => expect(mocks.complete).toHaveBeenCalledWith("failure", {
      worldDriverId: "ios", error: "Browser unavailable",
    }));
  });
});
