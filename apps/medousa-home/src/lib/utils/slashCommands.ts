/** Parse chat composer slash commands (budget, help, ask). */

export type SlashCommandResult =
  | { kind: "ask"; prompt: string }
  | { kind: "budget_list" }
  | { kind: "budget_approve"; requestId?: string }
  | { kind: "budget_deny"; requestId?: string }
  | { kind: "help" };

export function parseSlashCommand(value: string): SlashCommandResult | null {
  const trimmed = value.trim();
  if (trimmed.startsWith("/ask ")) {
    const prompt = trimmed.slice(5).trim();
    return prompt ? { kind: "ask", prompt } : null;
  }
  if (trimmed.startsWith("/daemon ask ")) {
    const prompt = trimmed.slice(12).trim();
    return prompt ? { kind: "ask", prompt } : null;
  }
  if (trimmed === "/help" || trimmed === "/commands" || trimmed === "/?") {
    return { kind: "help" };
  }
  if (trimmed === "/budget" || trimmed === "/budget list") {
    return { kind: "budget_list" };
  }
  if (trimmed.startsWith("/budget approve")) {
    const requestId = trimmed.slice("/budget approve".length).trim();
    return { kind: "budget_approve", requestId: requestId || undefined };
  }
  if (trimmed.startsWith("/budget deny")) {
    const requestId = trimmed.slice("/budget deny".length).trim();
    return { kind: "budget_deny", requestId: requestId || undefined };
  }
  return null;
}

export const SLASH_COMMAND_HINTS = [
  "/ask … — background job",
  "/budget list — pending round approvals",
  "/budget approve [id] — grant more tool rounds",
  "/budget deny [id] — stop the turn",
  "/help — show commands",
];
