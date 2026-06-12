import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";
import type {
  WizardAdvanceRequest,
  WizardBootstrap,
  WizardLocalState,
  WizardScreen,
  WIZARD_STORAGE_KEY,
} from "$lib/types/wizard";

const STORAGE_KEY: typeof WIZARD_STORAGE_KEY = "medousa-home-wizard-state";

function loadLocalState(): WizardLocalState | null {
  if (typeof localStorage === "undefined") return null;
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as WizardLocalState;
  } catch {
    return null;
  }
}

function saveLocalState(state: WizardLocalState): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

function localBootstrap(): WizardBootstrap {
  const existing = loadLocalState();
  if (existing?.state === "completed" && !existing.rerun) {
    return {
      visible: false,
      mode: "none",
      screen: "screen1",
    };
  }
  if (existing?.state === "active") {
    return {
      visible: true,
      mode: existing.mode,
      screen: existing.screen,
      existingProvider: existing.screen1Model,
    };
  }
  const fresh: WizardLocalState = {
    state: "active",
    screen: "screen1",
    mode: "fresh",
    rerun: false,
  };
  saveLocalState(fresh);
  return {
    visible: true,
    mode: "fresh",
    screen: "screen1",
  };
}

function localAdvance(request: WizardAdvanceRequest): WizardBootstrap {
  const current =
    loadLocalState() ??
    ({
      state: "active",
      screen: "screen1",
      mode: "fresh",
    } satisfies WizardLocalState);

  const nextScreen = advanceScreen(current.screen, request.action, request);
  const next: WizardLocalState = {
    ...current,
    screen: nextScreen,
    screen2Skipped:
      request.screen2Skipped ??
      current.screen2Skipped ??
      (request.action === "skip" && current.screen === "screen2"),
    screen3Skipped:
      request.screen3Skipped ??
      current.screen3Skipped ??
      (request.action === "skip" && current.screen === "screen3"),
  };

  if (request.action === "continue" && current.screen === "completion") {
    next.state = "completed";
    next.completedAt = new Date().toISOString();
    next.rerun = false;
  }

  if (request.action === "continue" && current.screen === "migration") {
    next.state = "completed";
    next.completedAt = new Date().toISOString();
    next.screen = "completion";
    next.mode = "none";
  }

  saveLocalState(next);
  if (next.state === "completed") {
    return { visible: false, mode: "none", screen: "completion" };
  }
  return {
    visible: true,
    mode: next.mode,
    screen: next.screen,
    existingProvider: next.screen1Model,
  };
}

function advanceScreen(
  screen: WizardScreen,
  action: WizardAdvanceRequest["action"],
  request?: WizardAdvanceRequest,
): WizardScreen {
  if (action === "back") {
    if (screen === "screen2" || screen === "screen3") return "screen1";
    return screen;
  }
  if (action === "skip") {
    if (screen === "screen2") return "screen3";
    if (screen === "screen3") return "completion";
    return screen;
  }
  switch (screen) {
    case "migration":
      return "completion";
    case "screen1":
      if (request?.screen1Model?.trim() === "mobile-client") return "completion";
      return "screen3";
    case "screen2":
      return "screen3";
    case "screen3":
      return "completion";
    case "completion":
      return "completion";
    default:
      return screen;
  }
}

export async function bootstrapWizard(): Promise<WizardBootstrap> {
  if (!isTauri()) return localBootstrap();
  return invoke<WizardBootstrap>("wizard_bootstrap");
}

export async function beginWizardRerun(): Promise<WizardBootstrap> {
  if (!isTauri()) {
    saveLocalState({
      state: "active",
      screen: "screen1",
      mode: "rerun",
      rerun: true,
    });
    return { visible: true, mode: "rerun", screen: "screen1" };
  }
  return invoke<WizardBootstrap>("wizard_begin_rerun");
}

export async function advanceWizard(request: WizardAdvanceRequest): Promise<WizardBootstrap> {
  if (!isTauri()) return localAdvance(request);
  return invoke<WizardBootstrap>("wizard_advance", { request });
}

export async function completeWizard(): Promise<WizardBootstrap> {
  if (!isTauri()) {
    saveLocalState({
      state: "completed",
      screen: "completion",
      mode: "none",
      completedAt: new Date().toISOString(),
      rerun: false,
    });
    return { visible: false, mode: "none", screen: "completion" };
  }
  return invoke<WizardBootstrap>("wizard_complete");
}

export function resetWizardLocalState(): void {
  localStorage.removeItem(STORAGE_KEY);
}
