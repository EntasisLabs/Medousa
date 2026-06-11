export type WizardLifecycleState = "active" | "completed";

export type WizardScreen =
  | "migration"
  | "screen1"
  | "screen2"
  | "screen3"
  | "completion";

export type WizardMode = "none" | "fresh" | "migration" | "rerun";

export interface WizardBootstrap {
  visible: boolean;
  mode: WizardMode;
  screen: WizardScreen;
  existingProvider?: string | null;
  existingModel?: string | null;
}

export interface WizardAdvanceRequest {
  action: "continue" | "skip" | "back";
  screen1Model?: string | null;
  screen2Skipped?: boolean;
  screen3Skipped?: boolean;
}

export const WIZARD_STORAGE_KEY = "medousa-home-wizard-state";

export interface WizardLocalState {
  state: WizardLifecycleState;
  screen: WizardScreen;
  mode: WizardMode;
  completedAt?: string | null;
  screen1Model?: string | null;
  screen2Skipped?: boolean;
  screen3Skipped?: boolean;
  migrationFrom?: string | null;
  rerun?: boolean;
}
