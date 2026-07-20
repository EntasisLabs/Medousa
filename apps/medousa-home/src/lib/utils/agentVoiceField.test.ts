import { describe, expect, it } from "vitest";
import {
  displayVoiceAppendix,
  humanizeScheduleValidationError,
  isSkillYamlResidue,
} from "./agentVoiceField";

describe("agentVoiceField", () => {
  it("flags YAML frontmatter dumps", () => {
    const dump = `---
name: frontend-design
description: Guidance for distinctive UI
---
`;
    expect(isSkillYamlResidue(dump)).toBe(true);
    expect(displayVoiceAppendix(dump)).toBe("");
  });

  it("keeps human prose", () => {
    const prose = "Warm, concise, and visually opinionated.";
    expect(isSkillYamlResidue(prose)).toBe(false);
    expect(displayVoiceAppendix(prose)).toBe(prose);
  });

  it("humanizes tools.allow schedule errors", () => {
    expect(
      humanizeScheduleValidationError(
        "scheduled lane requires spec.tools.allow to be non-empty",
      ),
    ).toBe("Add at least one tool before scheduling.");
  });
});
