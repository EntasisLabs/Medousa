import type { WorkshopCommand } from "./types";
import { usageScoreBoost } from "./usage";

function fuzzyScore(query: string, text: string): number {
  if (!query) return 1;
  if (text.startsWith(query)) return 200 + query.length;
  if (text.includes(query)) return 120 + query.length;

  let queryIndex = 0;
  let streak = 0;
  let score = 0;
  for (let i = 0; i < text.length && queryIndex < query.length; i += 1) {
    if (text[i] === query[queryIndex]) {
      queryIndex += 1;
      streak += 1;
      score += 10 + streak;
    } else {
      streak = 0;
    }
  }
  return queryIndex === query.length ? score : 0;
}

function commandHaystack(command: WorkshopCommand): string {
  return [
    command.label,
    command.subtitle ?? "",
    command.hint ?? "",
    command.keywords ?? "",
    ...(command.aliases ?? []),
  ]
    .join(" ")
    .toLowerCase();
}

export function scoreCommand(command: WorkshopCommand, query: string): number {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) {
    let base = command.section === "suggested" ? 300 : command.section === "go" ? 100 : 50;
    base += usageScoreBoost(command.id);
    return base;
  }

  const haystack = commandHaystack(command);
  const labelScore = fuzzyScore(trimmed, command.label.toLowerCase());
  const bodyScore = fuzzyScore(trimmed, haystack);
  const score = Math.max(labelScore, bodyScore * 0.92);
  if (score <= 0) return 0;
  return score + usageScoreBoost(command.id);
}

export function filterAndSortCommands(
  commands: WorkshopCommand[],
  query: string,
  limit = 48,
): WorkshopCommand[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) {
    return commands.slice(0, limit);
  }

  return commands
    .map((command) => ({ command, score: scoreCommand(command, trimmed) }))
    .filter((row) => row.score > 0)
    .sort(
      (left, right) =>
        right.score - left.score || left.command.label.localeCompare(right.command.label),
    )
    .slice(0, limit)
    .map((row) => row.command);
}
