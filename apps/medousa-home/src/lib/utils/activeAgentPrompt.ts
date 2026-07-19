import { activeAgent } from "$lib/stores/activeAgent.svelte";

/** Prepend `/skill {id}` when an agent is selected and the prompt isn't already skill-routed. */
export function applyActiveAgentPrompt(prompt: string): string {
  const id = activeAgent.selectedManuscriptId?.trim();
  if (!id) return prompt;
  const trimmed = prompt.trim();
  if (trimmed.startsWith("/skill ")) return prompt;
  if (trimmed.startsWith("/")) return prompt;
  if (!trimmed) return `/skill ${id}`;
  return `/skill ${id}\n${trimmed}`;
}
