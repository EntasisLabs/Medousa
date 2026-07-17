/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it } from "vitest";
import {
  readVaultBuildAutoSave,
  readVaultBuildLineNumbers,
  readVaultBuildWordWrap,
  writeVaultBuildAutoSave,
  writeVaultBuildLineNumbers,
  writeVaultBuildWordWrap,
} from "./vaultPreferences";

const KEYS = [
  "medousa-vault-build-word-wrap",
  "medousa-vault-build-line-numbers",
  "medousa-vault-build-auto-save",
];

describe("vault build editor preferences", () => {
  afterEach(() => {
    for (const key of KEYS) localStorage.removeItem(key);
  });

  it("defaults wrap on, line numbers off, autosave on", () => {
    expect(readVaultBuildWordWrap()).toBe(true);
    expect(readVaultBuildLineNumbers()).toBe(false);
    expect(readVaultBuildAutoSave()).toBe(true);
  });

  it("persists toggles", () => {
    writeVaultBuildWordWrap(false);
    writeVaultBuildLineNumbers(true);
    writeVaultBuildAutoSave(false);
    expect(readVaultBuildWordWrap()).toBe(false);
    expect(readVaultBuildLineNumbers()).toBe(true);
    expect(readVaultBuildAutoSave()).toBe(false);
  });
});
