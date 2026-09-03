import { describe, expect, it } from "vitest";
import type { TuiDefaults } from "$lib/types/workshopDefaults";
import {
  applyModelSelection,
  pickerAllowsClear,
  pickerClearHint,
  pickerTitle,
  rowLabelForTarget,
} from "./modelAssignment";

const draft = {
  provider: "openai-codex",
  model: "gpt-5.6-sol",
  inferenceProfiles: {
    main: {
      provider: "openai-codex",
      model: "gpt-5.6-sol",
      fallbacks: [],
    },
    vision: null,
  },
} as TuiDefaults;

describe("model role copy", () => {
  it("describes the optional vision profile as an image backup", () => {
    const target = { type: "primary", profile: "vision" } as const;
    expect(pickerTitle(target)).toBe("Image backup");
    expect(rowLabelForTarget(draft, target, null)).toEqual({
      title: "Image backup",
      value: "Automatic",
      hint: "Uses the conversation model first",
    });
  });

  it("allows a dedicated image backup to be cleared", () => {
    const target = { type: "primary", profile: "vision" } as const;
    expect(pickerAllowsClear(target)).toBe(true);
    expect(pickerClearHint(target)).toContain("conversation model first");

    const withImageBackup = {
      ...draft,
      inferenceProfiles: {
        ...draft.inferenceProfiles,
        vision: {
          provider: "openai-codex",
          model: "gpt-5.6-sol",
          fallbacks: [],
        },
      },
    } as TuiDefaults;
    const cleared = applyModelSelection(withImageBackup, target, null);
    expect(cleared.inferenceProfiles?.vision).toBeNull();
    expect(cleared.inferenceProfiles?.main).toEqual(draft.inferenceProfiles?.main);
  });
});
