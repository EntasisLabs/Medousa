import { describe, expect, it, vi } from "vitest";
import { signInChatGptOAuth, selectChatGptAccount, chatGptOAuthReady, type ChatGptOAuthConnection } from "./chatgptOAuth";

const invoke = vi.hoisted(() => vi.fn().mockResolvedValue({ status: "connected" }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("$lib/window", () => ({ isTauri: () => true }));

function connection(status: ChatGptOAuthConnection["status"]): ChatGptOAuthConnection {
  return { status, connected: status !== "signed_out" && status !== "reauth_required", plan_usage_enabled: status === "connected" || status === "refresh_required" };
}

describe("ChatGPT OAuth connection", () => {
  it("keeps the entire callback flow in one native command", async () => {
    await signInChatGptOAuth("oaiapp_saved", true);
    expect(invoke).toHaveBeenLastCalledWith("chatgpt_oauth_request", {
      operation: "sign_in", clientId: "oaiapp_saved", enablePlanUsage: true, authMethod: null,
    });
    await selectChatGptAccount("oaiapp_other");
    expect(invoke).toHaveBeenLastCalledWith("chatgpt_oauth_request", {
      operation: "select", clientId: "oaiapp_other", enablePlanUsage: false, authMethod: null,
    });
  });

  it("allows connected and refreshable accounts", () => {
    expect(chatGptOAuthReady(connection("connected"))).toBe(true);
    expect(chatGptOAuthReady(connection("refresh_required"))).toBe(true);
  });

  it("supports both sign-in methods without transferring a saved registration", async () => {
    await signInChatGptOAuth(undefined, false, "codex_legacy");
    expect(invoke).toHaveBeenLastCalledWith("chatgpt_oauth_request", {
      operation: "sign_in", clientId: null, enablePlanUsage: false, authMethod: "codex_legacy",
    });
    await signInChatGptOAuth("oaiapp_saved", true, "registered");
    expect(invoke).toHaveBeenLastCalledWith("chatgpt_oauth_request", {
      operation: "sign_in", clientId: "oaiapp_saved", enablePlanUsage: true, authMethod: "registered",
    });
  });

  it("blocks identity-only grants and old unverified connection states", () => {
    expect(chatGptOAuthReady(connection("plan_usage_disabled"))).toBe(false);
    expect(chatGptOAuthReady({ status: "connected", connected: true })).toBe(false);
    expect(chatGptOAuthReady({ status: "connected", connected: true, plan_usage_enabled: false })).toBe(false);
  });

  it("blocks signed-out and reauthentication-required accounts", () => {
    expect(chatGptOAuthReady(connection("signed_out"))).toBe(false);
    expect(chatGptOAuthReady(connection("reauth_required"))).toBe(false);
  });
});
