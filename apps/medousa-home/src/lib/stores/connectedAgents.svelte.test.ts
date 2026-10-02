import { describe, expect, it, vi } from "vitest";
import { ConnectedAgentStore } from "./connectedAgents.svelte";
import type { ExternalConversation } from "$lib/daemon/externalConversations";
describe("connected agent catalog", () => {
  it("shares in-flight requests and rejects stale workshop results", async () => {
    let finish!: (result: ExternalConversation[]) => void;
    const list = vi.fn().mockImplementationOnce(() => new Promise<ExternalConversation[]>((resolve) => finish = resolve)).mockResolvedValue([{id:"mini",label:"Mini"}]);
    const catalog = new ConnectedAgentStore(list);
    const first = catalog.refresh("mac");const duplicate = catalog.refresh("mac");
    expect(list).toHaveBeenCalledTimes(1);
    await catalog.refresh("mini"); finish([{id:"mac",label:"Mac"} as ExternalConversation]);
    await Promise.all([first,duplicate]); expect(catalog.conversations[0].id).toBe("mini");
    expect(catalog.workshopScopeId).toBe("mini");
  });
  it("retries a failed load and refreshes changed registrations", async () => {
    const list = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce([]).mockResolvedValueOnce([{id:"new"}]);
    const catalog = new ConnectedAgentStore(list);await catalog.refresh("mac");expect(catalog.error).toBe("offline");
    await catalog.refresh("mac");expect(catalog.error).toBeNull();await catalog.refresh("mac",true);expect(catalog.conversations[0].id).toBe("new");
  });
  it("reloads registrations created while the initial catalog was in flight", async () => {
    let finish!: (result: ExternalConversation[]) => void;
    const list = vi.fn()
      .mockImplementationOnce(() => new Promise<ExternalConversation[]>((resolve) => finish = resolve))
      .mockResolvedValueOnce([{id:"new-agent"}]);
    const catalog = new ConnectedAgentStore(list);
    const loading = catalog.refresh("mac");
    const changed = catalog.refresh("mac", true);
    const duplicateEvent = catalog.refresh("mac", true);
    finish([]);
    await Promise.all([loading, changed, duplicateEvent]);
    expect(list).toHaveBeenCalledTimes(2);
    expect(catalog.conversations[0].id).toBe("new-agent");
  });

});
