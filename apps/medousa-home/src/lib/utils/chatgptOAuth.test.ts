import { describe, expect, it } from "vitest";
import { chatGptOAuthReady, type ChatGptOAuthConnection } from "./chatgptOAuth";

function connection(status: ChatGptOAuthConnection["status"]): ChatGptOAuthConnection {
  return { status, connected: status !== "signed_out" && status !== "reauth_required" };
}

describe("ChatGPT OAuth connection", () => {
  it("allows connected and refreshable accounts", () => {
    expect(chatGptOAuthReady(connection("connected"))).toBe(true);
    expect(chatGptOAuthReady(connection("refresh_required"))).toBe(true);
  });

  it("blocks signed-out and reauthentication-required accounts", () => {
    expect(chatGptOAuthReady(connection("signed_out"))).toBe(false);
    expect(chatGptOAuthReady(connection("reauth_required"))).toBe(false);
  });
});
