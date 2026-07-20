/** First-session connections invite — shown once after +brain onboarding lands in Workspace. */

import { loadPreferredMode } from "$lib/utils/preferredMode";

const SEEN_KEY = "medousa-connections-invite-seen";
const ARMED_KEY = "medousa-connections-invite-armed";

export function armConnectionsInvite(): void {
  if (typeof localStorage === "undefined") return;
  if (localStorage.getItem(SEEN_KEY) === "true") return;
  if (loadPreferredMode() !== "workspace-ai") return;
  localStorage.setItem(ARMED_KEY, "true");
}

export function shouldShowConnectionsInvite(): boolean {
  if (typeof localStorage === "undefined") return false;
  if (localStorage.getItem(SEEN_KEY) === "true") return false;
  if (loadPreferredMode() !== "workspace-ai") return false;
  return localStorage.getItem(ARMED_KEY) === "true";
}

export function dismissConnectionsInvite(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(SEEN_KEY, "true");
  localStorage.removeItem(ARMED_KEY);
}

export function resetConnectionsInvite(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(SEEN_KEY);
  localStorage.removeItem(ARMED_KEY);
}
