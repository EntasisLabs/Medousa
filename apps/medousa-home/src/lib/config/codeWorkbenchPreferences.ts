/** Human Code workbench behavior preferences stored on this device. */

const STORAGE_KEY = "medousa-code-workbench-preferences-v1";

export type CodeAutosaveMode = "off" | "afterDelay";
export type CodeRunSavePolicy = "saveAll" | "requireClean";

export type CodeWorkbenchPreferences = {
  formatOnSave: boolean;
  autosave: CodeAutosaveMode;
  runSavePolicy: CodeRunSavePolicy;
  panelOnFailure: boolean;
};

export const DEFAULT_CODE_WORKBENCH_PREFERENCES: CodeWorkbenchPreferences = {
  formatOnSave: false,
  autosave: "off",
  runSavePolicy: "saveAll",
  panelOnFailure: true,
};

let memoryPreferences = { ...DEFAULT_CODE_WORKBENCH_PREFERENCES };

function validAutosave(value: unknown): value is CodeAutosaveMode {
  return value === "off" || value === "afterDelay";
}

function validRunSavePolicy(value: unknown): value is CodeRunSavePolicy {
  return value === "saveAll" || value === "requireClean";
}

export function readCodeWorkbenchPreferences(): CodeWorkbenchPreferences {
  if (typeof localStorage === "undefined") return { ...memoryPreferences };
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...memoryPreferences };
    const parsed = JSON.parse(raw) as Partial<CodeWorkbenchPreferences>;
    return {
      formatOnSave:
        typeof parsed.formatOnSave === "boolean"
          ? parsed.formatOnSave
          : DEFAULT_CODE_WORKBENCH_PREFERENCES.formatOnSave,
      autosave: validAutosave(parsed.autosave)
        ? parsed.autosave
        : DEFAULT_CODE_WORKBENCH_PREFERENCES.autosave,
      runSavePolicy: validRunSavePolicy(parsed.runSavePolicy)
        ? parsed.runSavePolicy
        : DEFAULT_CODE_WORKBENCH_PREFERENCES.runSavePolicy,
      panelOnFailure:
        typeof parsed.panelOnFailure === "boolean"
          ? parsed.panelOnFailure
          : DEFAULT_CODE_WORKBENCH_PREFERENCES.panelOnFailure,
    };
  } catch {
    return { ...memoryPreferences };
  }
}

export function writeCodeWorkbenchPreferences(
  patch: Partial<CodeWorkbenchPreferences>,
): CodeWorkbenchPreferences {
  const current = readCodeWorkbenchPreferences();
  const next: CodeWorkbenchPreferences = {
    formatOnSave:
      typeof patch.formatOnSave === "boolean"
        ? patch.formatOnSave
        : current.formatOnSave,
    autosave: validAutosave(patch.autosave) ? patch.autosave : current.autosave,
    runSavePolicy: validRunSavePolicy(patch.runSavePolicy)
      ? patch.runSavePolicy
      : current.runSavePolicy,
    panelOnFailure:
      typeof patch.panelOnFailure === "boolean"
        ? patch.panelOnFailure
        : current.panelOnFailure,
  };
  memoryPreferences = { ...next };
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    window.dispatchEvent(new CustomEvent("medousa-code-preferences-changed"));
  }
  return next;
}

export function resetCodeWorkbenchPreferences(): CodeWorkbenchPreferences {
  memoryPreferences = { ...DEFAULT_CODE_WORKBENCH_PREFERENCES };
  if (typeof localStorage !== "undefined") {
    localStorage.removeItem(STORAGE_KEY);
    window.dispatchEvent(new CustomEvent("medousa-code-preferences-changed"));
  }
  return { ...memoryPreferences };
}
