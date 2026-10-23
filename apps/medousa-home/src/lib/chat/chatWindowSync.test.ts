/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { bindChatWindowSync, publishChatWindowEvent } from "./chatWindowSync";
import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";

afterEach(() => { setActiveWorkshopIdPort(null); vi.unstubAllGlobals(); });
it("accepts receipts only from its workshop and publishes with the owning scope", () => {
  class Bus {
    static current: Bus;
    onmessage: ((message: { data: unknown }) => void) | null = null;
    close = vi.fn(); postMessage = vi.fn();
    constructor() { Bus.current = this; }
  }
  vi.stubGlobal("window", { BroadcastChannel: Bus });
  const host = { workshopScopeId: "studio", applyStreamEvent: vi.fn() };
  const stop = bindChatWindowSync(host), bus = Bus.current;
  const envelope = { turn_id: "turn", seq: 1, event: { type: "turn_completed" } } as TurnStreamEnvelopeV3;
  bus.onmessage!({ data: { scope: "other", envelope } }); expect(host.applyStreamEvent).not.toHaveBeenCalled();
  bus.onmessage!({ data: { scope: "studio", envelope } }); expect(host.applyStreamEvent).toHaveBeenCalledWith(envelope);
  bus.onmessage!({ data: { scope: "studio", envelope: { turn_id: "bad", seq: 2 } } }); expect(host.applyStreamEvent).toHaveBeenCalledOnce();
  setActiveWorkshopIdPort(() => "studio"); publishChatWindowEvent(envelope);
  expect(bus.postMessage).toHaveBeenCalledWith({ scope: "studio", envelope });
  stop(); expect(bus.close).toHaveBeenCalledOnce();
});
