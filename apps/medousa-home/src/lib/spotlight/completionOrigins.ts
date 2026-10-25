import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";

export const SPOTLIGHT_REPLIES_KEY = "medousa-home-notify-spotlight-replies";
export interface SpotlightTurnOrigin {
  workshopId: string;
  sessionId: string;
  turnId: string;
  acceptedAt: string;
  label: string;
  subjectIds?: string[];
}

function origins(workshopId: string): SpotlightTurnOrigin[] {
  try {
    const values: unknown = JSON.parse(localStorage.getItem(workshopScopedStorageKey("medousa-spotlight-turn-origins", workshopId)) ?? "[]");
    return Array.isArray(values) ? values.filter((value): value is SpotlightTurnOrigin =>
      value && ["workshopId", "sessionId", "turnId", "acceptedAt", "label"].every(key => typeof value[key] === "string")
      && value.workshopId === workshopId) : [];
  } catch { return []; }
}

/** Shared by windows; hiding Spotlight does not discard an admitted request. */
export function rememberSpotlightTurn(origin: SpotlightTurnOrigin): void {
  if (!origin.workshopId || !origin.sessionId || !origin.turnId) return;
  try {
    const values = origins(origin.workshopId).filter(value => value.turnId !== origin.turnId);
    localStorage.setItem(workshopScopedStorageKey("medousa-spotlight-turn-origins", origin.workshopId), JSON.stringify([...values, origin].slice(-512)));
  } catch { /* Notification bookkeeping must not block sending a request. */ }
}

export function spotlightTurnOrigin(workshopId: string, turnId: string, emittedAt: string): SpotlightTurnOrigin | null {
  const origin = origins(workshopId).find(value => value.turnId === turnId || (Array.isArray(value.subjectIds) && value.subjectIds.includes(turnId)));
  // External agents may reuse their stream ID for subsequent prompts.
  return origin && Date.parse(emittedAt) >= Date.parse(origin.acceptedAt) ? origin : null;
}
