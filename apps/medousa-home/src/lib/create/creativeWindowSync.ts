/** Invalidate identities, never transfer mutable scenes or assets between views. */
type Change = { scope: string; id: string };
const listeners = new Set<(change: Change) => void>();
let channel: BroadcastChannel | null = null;
function bus() {
  if (!channel && typeof window !== "undefined" && typeof window.BroadcastChannel === "function") {
    channel = new window.BroadcastChannel("medousa-create-saved-v1");
    channel.onmessage = event => {
      const value = event.data;
      if (typeof value?.scope !== "string" || typeof value?.id !== "string" || value.scope.length > 256 || value.id.length > 1024) return;
      listeners.forEach(listener => listener(value));
    };
  }
  return channel;
}
export function publishCreativeChange(scope: string, id: string) { bus()?.postMessage({ scope, id }); }
export function onCreativeWindowChange(listener: (change: Change) => void) {
  listeners.add(listener); bus();
  return () => { listeners.delete(listener); if (!listeners.size) { channel?.close(); channel = null; } };
}
