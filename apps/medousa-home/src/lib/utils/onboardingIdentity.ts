/** First-run personal names — local until daemon can absorb them. */

const PRINCIPAL_NAME_KEY = "medousa-onboarding-principal-name";
const ASSISTANT_NAME_KEY = "medousa-onboarding-assistant-name";

export function loadPrincipalName(): string {
  if (typeof localStorage === "undefined") return "";
  return localStorage.getItem(PRINCIPAL_NAME_KEY)?.trim() ?? "";
}

export function savePrincipalName(name: string): void {
  if (typeof localStorage === "undefined") return;
  const trimmed = name.trim();
  if (!trimmed) {
    localStorage.removeItem(PRINCIPAL_NAME_KEY);
    return;
  }
  localStorage.setItem(PRINCIPAL_NAME_KEY, trimmed);
}

export function loadAssistantName(): string {
  if (typeof localStorage === "undefined") return "";
  return localStorage.getItem(ASSISTANT_NAME_KEY)?.trim() ?? "";
}

export function saveAssistantName(name: string): void {
  if (typeof localStorage === "undefined") return;
  const trimmed = name.trim();
  if (!trimmed) {
    localStorage.removeItem(ASSISTANT_NAME_KEY);
    return;
  }
  localStorage.setItem(ASSISTANT_NAME_KEY, trimmed);
}

export function clearOnboardingIdentity(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(PRINCIPAL_NAME_KEY);
  localStorage.removeItem(ASSISTANT_NAME_KEY);
}
