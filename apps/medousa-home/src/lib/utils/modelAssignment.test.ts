import { describe, expect, it } from "vitest";
import type { TuiDefaults } from "$lib/types/workshopDefaults";
import {
  applyModelSelection,
  pickerAllowsClear,
  pickerClearHint,
  pickerTitle,
  providerIdsForTarget,
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
    imageGeneration: null,
  },
} as TuiDefaults;

describe("model role copy", () => {
  it("describes the optional vision profile directly", () => {
    const target = { type: "primary", profile: "vision" } as const;
    expect(pickerTitle(target)).toBe("Vision model");
    expect(rowLabelForTarget(draft, target, null)).toEqual({
      title: "Vision model",
      value: "Automatic",
      hint: "Uses the conversation model first",
    });
  });

  it("keeps image generation as an explicit clearable role", () => {
    const target = { type: "primary", profile: "imageGeneration" } as const;
    expect(pickerTitle(target)).toBe("Image generation");
    expect(pickerAllowsClear(target)).toBe(true);
    expect(pickerClearHint(target)).toBe("Turn off image generation");
    expect(providerIdsForTarget(target)).toEqual(["openai"]);
    expect(rowLabelForTarget(draft, target, null).value).toBe("Not set");

    const selected = applyModelSelection(draft, target, {
      provider: "openai",
      model: "gpt-image-2",
      baseUrl: null,
    });
    expect(selected.inferenceProfiles?.imageGeneration?.model).toBe("gpt-image-2");
    expect(applyModelSelection(selected, target, null).inferenceProfiles?.imageGeneration).toBeNull();
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
