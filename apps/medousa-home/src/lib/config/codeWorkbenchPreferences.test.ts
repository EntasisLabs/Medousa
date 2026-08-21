import { beforeEach, describe, expect, it } from "vitest";

import {
  DEFAULT_CODE_WORKBENCH_PREFERENCES,
  readCodeWorkbenchPreferences,
  resetCodeWorkbenchPreferences,
  writeCodeWorkbenchPreferences,
} from "./codeWorkbenchPreferences";

describe("Code workbench preferences", () => {
  beforeEach(() => resetCodeWorkbenchPreferences());

  it("uses conservative human-coding defaults", () => {
    expect(readCodeWorkbenchPreferences()).toEqual(
      DEFAULT_CODE_WORKBENCH_PREFERENCES,
    );
  });

  it("persists only bounded values", () => {
    writeCodeWorkbenchPreferences({
      formatOnSave: true,
      autosave: "afterDelay",
      runSavePolicy: "requireClean",
      panelOnFailure: false,
    });
    expect(readCodeWorkbenchPreferences()).toEqual({
      formatOnSave: true,
      autosave: "afterDelay",
      runSavePolicy: "requireClean",
      panelOnFailure: false,
    });

    writeCodeWorkbenchPreferences({
      autosave: "sometimes" as never,
      runSavePolicy: "stale" as never,
    });
    expect(readCodeWorkbenchPreferences().autosave).toBe("afterDelay");
    expect(readCodeWorkbenchPreferences().runSavePolicy).toBe("requireClean");
  });
});
