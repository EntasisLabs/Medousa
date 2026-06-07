export interface EnqueueAskJobRequest {
  prompt: string;
  modelHint?: string;
  manuscriptId?: string;
  additionalManuscriptIds?: string[];
  suggestedCapabilityIds?: string[];
}

export function buildAskJobRequest(
  prompt: string,
  skillIds: string[],
  capabilityIds: string[],
): EnqueueAskJobRequest {
  const trimmed = prompt.trim();
  return {
    prompt: trimmed,
    manuscriptId: skillIds[0],
    additionalManuscriptIds:
      skillIds.length > 1 ? skillIds.slice(1) : undefined,
    suggestedCapabilityIds: capabilityIds.length ? capabilityIds : undefined,
  };
}

export function canSubmitAskJob(
  prompt: string,
  skillIds: string[],
): boolean {
  return prompt.trim().length > 0 || skillIds.length > 0;
}

export function suggestedRunnableSkills<
  T extends { has_scripts: boolean; name: string },
>(entries: T[], limit = 6): T[] {
  return [...entries]
    .filter((entry) => entry.has_scripts)
    .sort((left, right) => left.name.localeCompare(right.name))
    .slice(0, limit);
}

export function suggestedTools<T extends { title: string }>(
  entries: T[],
  limit = 6,
): T[] {
  return [...entries]
    .sort((left, right) => left.title.localeCompare(right.title))
    .slice(0, limit);
}
