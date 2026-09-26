import { writable } from "svelte/store";
import type { LiveDeepLink } from "./deepLinks";

// Retain cold-start navigation until the Live control has mounted. Only the
// latest launch is pending; repeat deliveries cannot create another chat.
export const pendingLiveLaunch = writable<LiveDeepLink | null>(null);
const delivered = new Set<string>();

export function queueLiveLaunch(request: LiveDeepLink): void {
  if (delivered.has(request.requestId)) return;
  delivered.add(request.requestId);
  if (delivered.size > 128) delivered.delete(delivered.values().next().value!);
  pendingLiveLaunch.set(request);
}
