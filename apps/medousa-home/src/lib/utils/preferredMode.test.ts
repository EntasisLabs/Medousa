/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it } from "vitest";
import {
  clearPreferredMode,
  isWorkspaceOnlyMode,
  loadPreferredMode,
  loadWizardPowersDone,
  markWizardPowersDone,
  resetWizardRelationshipFlags,
  savePreferredMode,
} from "./preferredMode";

describe("preferredMode", () => {
  beforeEach(() => {
    clearPreferredMode();
    resetWizardRelationshipFlags();
  });

  it("persists workspace vs workspace-ai", () => {
    expect(loadPreferredMode()).toBeNull();
    savePreferredMode("workspace");
    expect(loadPreferredMode()).toBe("workspace");
    expect(isWorkspaceOnlyMode()).toBe(true);
    savePreferredMode("workspace-ai");
    expect(isWorkspaceOnlyMode()).toBe(false);
  });

  it("tracks relationship flags for powers", () => {
    expect(loadWizardPowersDone()).toBe(false);
    markWizardPowersDone();
    expect(loadWizardPowersDone()).toBe(true);
    resetWizardRelationshipFlags();
    expect(loadWizardPowersDone()).toBe(false);
  });
});
