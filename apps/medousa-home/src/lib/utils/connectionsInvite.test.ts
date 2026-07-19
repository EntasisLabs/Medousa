/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it } from "vitest";
import {
  armConnectionsInvite,
  dismissConnectionsInvite,
  resetConnectionsInvite,
  shouldShowConnectionsInvite,
} from "./connectionsInvite";
import {
  clearPreferredMode,
  savePreferredMode,
} from "./preferredMode";

describe("connectionsInvite", () => {
  beforeEach(() => {
    resetConnectionsInvite();
    clearPreferredMode();
  });

  it("arms and shows only for workspace-ai path", () => {
    savePreferredMode("workspace");
    armConnectionsInvite();
    expect(shouldShowConnectionsInvite()).toBe(false);

    savePreferredMode("workspace-ai");
    armConnectionsInvite();
    expect(shouldShowConnectionsInvite()).toBe(true);

    dismissConnectionsInvite();
    expect(shouldShowConnectionsInvite()).toBe(false);
    armConnectionsInvite();
    expect(shouldShowConnectionsInvite()).toBe(false);
  });
});
