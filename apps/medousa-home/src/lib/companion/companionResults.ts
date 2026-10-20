import { conversationTargets, type ConversationTarget } from "$lib/spotlight/conversationTargets";
import type { BotProfile } from "$lib/types/generated/daemon_api";
import type { SessionSummary } from "$lib/types/session";
import type { AgentRuntimeInfo } from "$lib/daemon/session";

export interface CompanionAction {
  id: string;
  label: string;
  description?: string;
  keywords: string;
  icon: "chat" | "note" | "web" | "view" | "main";
  run: () => void | Promise<void>;
}
export type CompanionResult = {
  id: string; label: string; description: string; group: string;
} & ({ kind: "target"; target: ConversationTarget } | { kind: "action"; action: CompanionAction } | { kind: "ask" });

/** Search the same destinations as Spotlight, with the companion's existing tools. */
export function companionResults(query: string, scope: string, bots: BotProfile[], sessions: SessionSummary[], runtimes: AgentRuntimeInfo[], actions: CompanionAction[]): CompanionResult[] {
  const needle = query.trim().toLocaleLowerCase();
  const newRequest = needle === "new" || needle === "new request";
  const destinations = conversationTargets(scope, bots, sessions, runtimes, newRequest ? "medousa" : needle);
  const existing = destinations.filter(target => !newRequest && target.kind !== "new").sort((a, b) => Number(a.kind === "bot") - Number(b.kind === "bot")).slice(0, needle ? 6 : 3);
  const fresh = destinations.filter(target => target.kind === "new" && (needle || target.runtime === "medousa"));
  const toResult = (target: ConversationTarget): CompanionResult => ({
    id: target.id, kind: "target", target,
    label: target.kind === "new" && target.runtime === "medousa" ? "New request" : target.label,
    description: target.kind === "new" ? target.description : target.description || "Continue this conversation",
    group: target.kind === "new" ? "Create" : "Continue",
  });
  const matchingActions = actions.filter(action => !needle || needle.split(/\s+/).every(word => `${action.label} ${action.keywords}`.toLocaleLowerCase().includes(word))).slice(0, needle ? 8 : 2);
  const results: CompanionResult[] = [
    ...existing.map(toResult), ...fresh.map(toResult),
    ...matchingActions.map(action => ({ id: action.id, kind: "action" as const, action, label: action.label, description: action.description || "", group: "Actions" })),
  ];
  if (needle && !results.length) results.push({ id: "ask:medousa", kind: "ask", label: "Ask Medousa", description: query.trim(), group: "Ask" });
  return results;
}
