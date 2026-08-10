import { describe, expect, it } from "vitest";
import {
  canInvokeCodeSaveShortcut,
  decideCodeSave,
  type CodeSaveGateInput,
} from "./codeSaveGate";

const base: CodeSaveGateInput = {
  preview: false,
  dirty: true,
  savingFile: false,
  hasLease: true,
  canBeginEdit: false,
  beginningEdit: false,
};

describe("decideCodeSave", () => {
  it("saves when dirty and leased", () => {
    expect(decideCodeSave(base)).toEqual({ action: "save" });
  });

  it("noops when not dirty", () => {
    expect(decideCodeSave({ ...base, dirty: false })).toEqual({
      action: "noop",
      reason: "not-dirty",
    });
  });

  it("noops when a save is already in flight", () => {
    expect(decideCodeSave({ ...base, savingFile: true })).toEqual({
      action: "noop",
      reason: "already-saving",
    });
  });

  it("rejects preview files", () => {
    expect(decideCodeSave({ ...base, preview: true })).toEqual({
      action: "reject",
      reason: "preview",
    });
  });

  it("awaits an in-flight begin-edit (type-then-Cmd+S race)", () => {
    expect(
      decideCodeSave({
        ...base,
        hasLease: false,
        canBeginEdit: true,
        beginningEdit: true,
      }),
    ).toEqual({ action: "await-lease" });
  });

  it("begins a lease then saves when soft-lease is available", () => {
    expect(
      decideCodeSave({
        ...base,
        hasLease: false,
        canBeginEdit: true,
        beginningEdit: false,
      }),
    ).toEqual({ action: "begin-then-save" });
  });

  it("rejects when there is no lease and begin is not allowed", () => {
    expect(
      decideCodeSave({
        ...base,
        hasLease: false,
        canBeginEdit: false,
        beginningEdit: false,
      }),
    ).toEqual({ action: "reject", reason: "no-lease" });
  });

  it("prefers save over begin when lease is already present", () => {
    expect(
      decideCodeSave({
        ...base,
        hasLease: true,
        canBeginEdit: true,
        beginningEdit: true,
      }),
    ).toEqual({ action: "save" });
  });
});

describe("canInvokeCodeSaveShortcut", () => {
  it("allows when editable or soft-lease begin is available", () => {
    expect(canInvokeCodeSaveShortcut({ editable: true, canBeginEdit: false })).toBe(true);
    expect(canInvokeCodeSaveShortcut({ editable: false, canBeginEdit: true })).toBe(true);
    expect(canInvokeCodeSaveShortcut({ editable: false, canBeginEdit: false })).toBe(false);
  });
});
