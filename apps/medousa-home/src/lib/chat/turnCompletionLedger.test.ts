import { describe, expect, it } from "vitest";
import { TurnCompletionLedger } from "./turnCompletionLedger";
import { waitForLiveWork } from "$lib/liveWorkResult";

describe("chat completion receipts", () => {
  it("keeps the exact result after active turn deletion and history ID replacement", async () => {
    const ledger = new TurnCompletionLedger();
    const messages = [{ id: "stream-id", turnId: "turn-1", content: "Six MCP servers connected." }];
    ledger.record("phone", "chat", "turn-1", { terminal: true, phase: "done", text: messages[0].content });
    messages.splice(0, 1, { id: "chat:ent_saved", turnId: "", content: "Six MCP servers connected." });
    expect(ledger.read("phone", "chat", "turn-1")?.text).toBe("Six MCP servers connected.");
    expect(ledger.read("other-workshop", "chat", "turn-1")).toBeNull();
    expect(ledger.read("phone", "other-chat", "turn-1")).toBeNull();
    const result = await waitForLiveWork("turn-1", () => ledger.read("phone", "chat", "turn-1"), new AbortController().signal);
    expect(result).toEqual({ status: "completed", turnId: "turn-1", text: "Six MCP servers connected." });
  });

  it("expires receipts and bounds retained results", () => {
    let now = 0;
    const ledger = new TurnCompletionLedger(() => now);
    for (let index = 0; index <= 100; index++) ledger.record("scope", "session", String(index), {
      terminal: true, phase: "done", text: "x".repeat(20000),
    });
    expect(ledger.read("scope", "session", "0")).toBeNull();
    expect(ledger.read("scope", "session", "100")?.text.length).toBe(12000);
    now = 600001;
    expect(ledger.read("scope", "session", "100")).toBeNull();
  });
});
