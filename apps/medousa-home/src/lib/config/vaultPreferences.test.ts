/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it } from "vitest";
import {
  cycleVaultPaperWidth,
  readVaultBuildAutoSave,
  readVaultBuildLineNumbers,
  readVaultBuildScrollSync,
  readVaultBuildWordWrap,
  readVaultHideLiveMarkdownSyntax,
  readVaultPaperWidth,
  writeVaultBuildAutoSave,
  writeVaultBuildLineNumbers,
  writeVaultBuildScrollSync,
  writeVaultBuildWordWrap,
  writeVaultHideLiveMarkdownSyntax,
  writeVaultPaperWidth,
} from "./vaultPreferences";

const KEYS = [
  "medousa-vault-build-word-wrap",
  "medousa-vault-build-line-numbers",
  "medousa-vault-build-auto-save",
  "medousa-vault-build-scroll-sync",
  "medousa-vault-hide-live-markdown-syntax",
  "medousa-vault-paper-width",
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

  it("persists hide-live-syntax and paper width", () => {
    expect(readVaultHideLiveMarkdownSyntax()).toBe(false);
    expect(readVaultPaperWidth()).toBe("wide");
    writeVaultHideLiveMarkdownSyntax(true);
    writeVaultPaperWidth("narrow");
    expect(readVaultHideLiveMarkdownSyntax()).toBe(true);
    expect(readVaultPaperWidth()).toBe("narrow");
    expect(cycleVaultPaperWidth("narrow")).toBe("medium");
  });
});
