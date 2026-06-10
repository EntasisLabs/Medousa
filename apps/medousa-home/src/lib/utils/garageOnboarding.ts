/** M8f — first-run garage onboarding (localStorage only). */

export const GARAGE_ONBOARDING_COMPLETE_KEY = "medousa-home-garage-onboarding-complete";

export function loadGarageOnboardingComplete(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(GARAGE_ONBOARDING_COMPLETE_KEY) === "true";
}

export function completeGarageOnboarding(): void {
  localStorage.setItem(GARAGE_ONBOARDING_COMPLETE_KEY, "true");
}

export function resetGarageOnboarding(): void {
  localStorage.removeItem(GARAGE_ONBOARDING_COMPLETE_KEY);
}

export function shouldShowGarageWizard(): boolean {
  return !loadGarageOnboardingComplete();
}
