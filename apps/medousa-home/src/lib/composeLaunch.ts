import { writable } from "svelte/store";
import type { ComposeDeepLink } from "$lib/deepLinks";

// Retain cold-start widget actions until the mobile composer has mounted.
export const pendingComposeLaunch = writable<ComposeDeepLink | null>(null);
const delivered = new Set<string>();

export function queueComposeLaunch(request: ComposeDeepLink): void {
  if (delivered.has(request.requestId)) return;
  delivered.add(request.requestId);
  if (delivered.size > 128) delivered.delete(delivered.values().next().value!);
  pendingComposeLaunch.set(request);
}
