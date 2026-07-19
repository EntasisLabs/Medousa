/**
 * Detect SKILL.md / YAML frontmatter dumps that should not appear as "how she sounds".
 * Import may still store residue on disk — the UI treats it as empty prose.
 */
export function isSkillYamlResidue(value: string | null | undefined): boolean {
  const text = value?.trim() ?? "";
  if (!text) return false;

  if (text.startsWith("---")) return true;

  const lines = text.split(/\r?\n/).slice(0, 12);
  let yamlKeys = 0;
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    if (/^(name|description|license|compatibility|metadata|allowed-tools)\s*:/i.test(trimmed)) {
      yamlKeys += 1;
    }
  }
  return yamlKeys >= 2;
}

/** Voice field value for the editor — empty when residue would cliff the climax. */
export function displayVoiceAppendix(value: string | null | undefined): string {
  if (isSkillYamlResidue(value)) return "";
  return value?.trim() ? value : "";
}

export function humanizeScheduleValidationError(error: string | null | undefined): string {
  const raw = error?.trim() ?? "";
  if (!raw) return "";
  if (/tools\.allow|tools_allow|non-empty/i.test(raw)) {
    return "Add at least one tool before scheduling.";
  }
  if (/identity_remember/i.test(raw)) {
    return "This agent’s tools can’t run on a schedule as configured.";
  }
  if (/openshell|skill sandbox/i.test(raw)) {
    return "Sandbox tools need permission before they can run on a schedule.";
  }
  if (/shell/i.test(raw) && /denied|scheduled/i.test(raw)) {
    return "Shell tools aren’t allowed on scheduled runs.";
  }
  return raw;
}
