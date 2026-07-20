/** First-run preferred mode — Workspace (AI-optional) vs Workspace + brain. */

export type PreferredMode = "workspace" | "workspace-ai";

const PREFERRED_MODE_KEY = "medousa-preferred-mode";
const WIZARD_POWERS_DONE_KEY = "medousa-wizard-powers-done";
const WIZARD_TOUR_DONE_KEY = "medousa-wizard-tour-done";
/** Passed the arrival beat (atmosphere). */
const WIZARD_TRUST_DONE_KEY = "medousa-wizard-trust-done";
/** Named + colored the space. */
const WIZARD_SPACE_DONE_KEY = "medousa-wizard-space-done";

export function loadPreferredMode(): PreferredMode | null {
  if (typeof localStorage === "undefined") return null;
  const raw = localStorage.getItem(PREFERRED_MODE_KEY);
  if (raw === "workspace" || raw === "workspace-ai") return raw;
  return null;
}

export function savePreferredMode(mode: PreferredMode): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(PREFERRED_MODE_KEY, mode);
}

export function clearPreferredMode(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(PREFERRED_MODE_KEY);
}

export function isWorkspaceOnlyMode(): boolean {
  return loadPreferredMode() === "workspace";
}

export function loadWizardTrustDone(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(WIZARD_TRUST_DONE_KEY) === "true";
}

export function markWizardTrustDone(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(WIZARD_TRUST_DONE_KEY, "true");
}

export function loadWizardSpaceDone(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(WIZARD_SPACE_DONE_KEY) === "true";
}

export function markWizardSpaceDone(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(WIZARD_SPACE_DONE_KEY, "true");
}

export function loadWizardPowersDone(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(WIZARD_POWERS_DONE_KEY) === "true";
}

export function markWizardPowersDone(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(WIZARD_POWERS_DONE_KEY, "true");
}

export function loadWizardTourDone(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(WIZARD_TOUR_DONE_KEY) === "true";
}

export function markWizardTourDone(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(WIZARD_TOUR_DONE_KEY, "true");
}

export function resetWizardRelationshipFlags(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(WIZARD_TRUST_DONE_KEY);
  localStorage.removeItem(WIZARD_SPACE_DONE_KEY);
  localStorage.removeItem(WIZARD_POWERS_DONE_KEY);
  localStorage.removeItem(WIZARD_TOUR_DONE_KEY);
}
