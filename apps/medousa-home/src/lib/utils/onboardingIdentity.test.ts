/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it } from "vitest";
import {
  clearOnboardingIdentity,
  loadAssistantName,
  loadPrincipalName,
  saveAssistantName,
  savePrincipalName,
} from "./onboardingIdentity";

describe("onboardingIdentity", () => {
  beforeEach(() => {
    clearOnboardingIdentity();
  });

  it("persists principal and assistant names", () => {
    expect(loadPrincipalName()).toBe("");
    expect(loadAssistantName()).toBe("");
    savePrincipalName("Alex");
    saveAssistantName("Medousa");
    expect(loadPrincipalName()).toBe("Alex");
    expect(loadAssistantName()).toBe("Medousa");
  });
});
