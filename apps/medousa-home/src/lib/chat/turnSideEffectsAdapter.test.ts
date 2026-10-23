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

import { v3PresentationEvent } from "$lib/stream/v3PresentationAdapter";
import { handlePermissionRequest, handleBrowserChallenge } from "./turnSideEffectsAdapter";

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

describe("typed external-agent permissions", () => {
  it("preserves exact provider options from the V3 stream into the permission bar state", () => {
    const options = [
      { option_id: "allow-all", name: "Always allow", kind: "allow_always" },
      { option_id: "deny-once", name: "No thanks", kind: "reject_once" },
    ];
    const presentation = v3PresentationEvent({
      schema_version: 3, turn_id: "turn", seq: 1, emitted_at_utc: "2026-10-23T00:00:00Z",
      event: { type: "permission_request", request_id: "request", agent_runtime: "pi", agent_session_id: "agent", message: "Choose a permission", permission_options: options },
    });
    const host = { permissionAlert: null, messageIdForTurn: () => "message" } as unknown as ChatStoreHost;
    handlePermissionRequest(host, presentation);
    expect(host.permissionAlert).toMatchObject({ requestId: "request", agentRuntime: "pi", options });
  });
  it("keeps typed stream events from older workshops compatible with Allow/Deny", () => {
    const presentation = v3PresentationEvent({
      schema_version: 3, turn_id: "turn", seq: 1, emitted_at_utc: "2026-10-23T00:00:00Z",
      event: { type: "permission_request", request_id: "old-request", message: "Allow this operation?" },
    });
    const host = { permissionAlert: null, messageIdForTurn: () => "message" } as unknown as ChatStoreHost;
    handlePermissionRequest(host, presentation);
    expect(host.permissionAlert?.options).toEqual([]);
  });
});
