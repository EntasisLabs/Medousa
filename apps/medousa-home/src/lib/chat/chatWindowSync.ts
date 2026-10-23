import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
import { activeWorkshopId } from "$lib/utils/workshopLocality";

let channel: BroadcastChannel | null = null;
/** Stream envelopes are immutable daemon receipts; each receiver still applies
 * its usual sequence/session guards. No transport ownership is transferred. */
export function publishChatWindowEvent(envelope: TurnStreamEnvelopeV3) {
  channel?.postMessage({ scope: activeWorkshopId(), envelope });
}
export function bindChatWindowSync(host: { workshopScopeId: string | null; applyStreamEvent: (event: TurnStreamEnvelopeV3) => void }) {
  if (typeof window.BroadcastChannel !== "function") return () => {};
  channel?.close();
  const bus = new window.BroadcastChannel("medousa-chat-events-v1");
  channel = bus;
  bus.onmessage = event => {
    const value = event.data;
    if (value?.scope === host.workshopScopeId && typeof value?.envelope?.turn_id === "string" && typeof value?.envelope?.seq === "number" && typeof value?.envelope?.event?.type === "string") host.applyStreamEvent(value.envelope);
  };
  return () => { bus.close(); if (channel === bus) channel = null; };
}
