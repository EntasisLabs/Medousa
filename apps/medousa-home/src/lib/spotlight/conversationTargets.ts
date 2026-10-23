import { EXTERNAL_AGENTS } from "$lib/utils/externalAgentCatalog";
import type { ExternalAgentRuntime } from "$lib/utils/externalAgentCatalog";
import type { BotProfile } from "$lib/types/generated/daemon_api";
import type { SessionSummary } from "$lib/types/session";
import type { AgentRuntimeInfo } from "$lib/daemon/session";
import { formatSessionLabel } from "$lib/utils/formatSession";
import { getSessionAgentRuntime, isProviderConversationRuntime } from "$lib/utils/sessionAgentRuntime";

export type ConversationTarget = {
  id: string;
  label: string;
  description: string;
  group: "Bots" | "Conversations" | "Start new";
  scope: string;
} & ({ kind: "bot"; bot: BotProfile } | { kind: "session"; sessionId: string } | { kind: "new"; runtime: "medousa" | ExternalAgentRuntime });

/** Stable addresses, not display names: two sessions may share a name. */
export function conversationTargets(scope: string, bots: BotProfile[], sessions: SessionSummary[], runtimes: AgentRuntimeInfo[], query: string): ConversationTarget[] {
  const botSessions = new Set(bots.map(bot => bot.primary_session_id));
  const targets: ConversationTarget[] = [
    ...bots.filter(bot => !bot.archived).map(bot => ({ id: `bot:${bot.bot_id}`, label: bot.display_name, description: bot.role_description || "Continue with this bot", group: "Bots" as const, scope, kind: "bot" as const, bot })),
    ...sessions.filter(session => !botSessions.has(session.session_id) && !isProviderConversationRuntime(getSessionAgentRuntime(session.session_id))).map(session => ({ id: `session:${session.session_id}`, label: formatSessionLabel(session), description: session.preview || "Continue this conversation", group: "Conversations" as const, scope, kind: "session" as const, sessionId: session.session_id })),
    { id: "new:medousa", label: "Medousa", description: "Start a new assistant conversation", group: "Start new", scope, kind: "new", runtime: "medousa" },
    ...EXTERNAL_AGENTS.filter(agent => runtimes.some(item => item.runtime === agent.id && item.available && item.auth_status !== "signed_out")).map(agent => ({ id: `new:${agent.id}`, label: agent.label, description: "Start a new agent session", group: "Start new" as const, scope, kind: "new" as const, runtime: agent.id })),
  ];
  const needle = query.trim().toLocaleLowerCase();
  const groups: ConversationTarget["group"][] = ["Bots", "Conversations", "Start new"];
  const exact = (target: ConversationTarget) => Boolean(needle) && target.label.toLocaleLowerCase() === needle;
  // Exact addresses win over partial names (e.g. @Medousa vs Medousa Coder).
  groups.sort((a, b) => Number(targets.some(target => target.group === b && exact(target))) - Number(targets.some(target => target.group === a && exact(target))));
  return groups.flatMap(group => targets.filter(target => target.group === group && (!needle || target.label.toLocaleLowerCase().includes(needle))).sort((a, b) => Number(exact(b)) - Number(exact(a))).slice(0, 12));
}

export function mentionQuery(query: string): { name: string; message: string } | null {
  const match = /^@([^\s]*)(?:\s+([\s\S]*))?$/.exec(query);
  return match ? { name: match[1], message: match[2] ?? "" } : null;
}
