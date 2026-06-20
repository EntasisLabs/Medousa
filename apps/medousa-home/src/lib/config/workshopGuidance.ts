/** Workshop UX guidance — journey steps, recipe prominence, friendly run summaries. */

const JOURNEY_DISMISSED_KEY = "medousa-workshop-journey-dismissed";

export function isWorkshopJourneyDismissed(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(JOURNEY_DISMISSED_KEY) === "1";
}

export function dismissWorkshopJourney(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(JOURNEY_DISMISSED_KEY, "1");
}

export function resetWorkshopJourneyDismissed(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(JOURNEY_DISMISSED_KEY);
}
