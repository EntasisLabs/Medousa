import type { HomeNotificationKind } from "$lib/types/workspace";

export type HomeNotificationPreferences = {
  turnUpdates: boolean;
  needsInput: boolean;
};

/** Category policy shared by local presentation tests and the live presenter. */
export function shouldPresentHomeNotification(
  kind: HomeNotificationKind,
  preferences: HomeNotificationPreferences,
): boolean {
  if (kind === "fatal_turn" || kind === "scheduled_delivery") return true;
  if (kind === "turn_update") return preferences.turnUpdates;
  return preferences.needsInput;
}
