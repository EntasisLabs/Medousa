/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it } from "vitest";
import {
  readVaultBuildAutoSave,
  readVaultBuildLineNumbers,
  readVaultBuildScrollSync,
  readVaultBuildWordWrap,
  writeVaultBuildAutoSave,
  writeVaultBuildLineNumbers,
  writeVaultBuildScrollSync,
  writeVaultBuildWordWrap,
} from "./vaultPreferences";

const KEYS = [
  "medousa-vault-build-word-wrap",
  "medousa-vault-build-line-numbers",
  "medousa-vault-build-auto-save",
  "medousa-vault-build-scroll-sync",
];

describe("vault build editor preferences", () => {
  afterEach(() => {
    for (const key of KEYS) localStorage.removeItem(key);
  });

  it("defaults wrap on, line numbers off, autosave on, scroll sync on", () => {
    expect(readVaultBuildWordWrap()).toBe(true);
    expect(readVaultBuildLineNumbers()).toBe(false);
    expect(readVaultBuildAutoSave()).toBe(true);
    expect(readVaultBuildScrollSync()).toBe(true);
  });

  it("persists toggles", () => {
    writeVaultBuildWordWrap(false);
    writeVaultBuildLineNumbers(true);
    writeVaultBuildAutoSave(false);
    writeVaultBuildScrollSync(false);
    expect(readVaultBuildWordWrap()).toBe(false);
    expect(readVaultBuildLineNumbers()).toBe(true);
    expect(readVaultBuildAutoSave()).toBe(false);
    expect(readVaultBuildScrollSync()).toBe(false);
  });
});
