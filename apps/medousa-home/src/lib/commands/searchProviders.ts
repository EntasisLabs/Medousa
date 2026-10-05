import { bots } from "$lib/stores/bots.svelte";
import { connectedAgents } from "$lib/stores/connectedAgents.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { agentRuntimeLabel, getSessionAgentRuntime, isProviderConversationRuntime } from "$lib/utils/sessionAgentRuntime";
import { externalConversationSessionId } from "$lib/utils/externalConversationSession";
import { chat } from "$lib/stores/chat.svelte";
import { browserHistory } from "$lib/browser/browserHistory.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { fuzzyMatchVaultNotes } from "$lib/utils/vaultFuzzyMatch";
import { formatSessionLabel } from "$lib/utils/formatSession";
import type { WorkshopCommand, WorkshopCommandContext } from "./types";
import { columnLabel } from "$lib/types/workspace";

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

export function buildNoteOpenCommands(
  _ctx: WorkshopCommandContext,
  query: string,
  limit = 12,
): WorkshopCommand[] {
  const labelByPath = vault.labelByPathMap;
  const notes = fuzzyMatchVaultNotes(vault.notes, query, labelByPath, limit);
  return notes.map((note) => {
    const title = labelByPath.get(note.path) ?? note.title;
    return {
      id: `open-note:${note.path}`,
      section: "open" as const,
      label: `Open note: ${title}`,
      subtitle: note.path,
      keywords: `note vault ${note.path} ${title}`,
      run: async (runCtx) => {
        await lmeWorkspace.openNote(note.path);
        runCtx.callbacks.close();
      },
    };
  });
}

export function buildSessionOpenCommands(
  ctx: WorkshopCommandContext,
  query: string,
  limit = 8,
): WorkshopCommand[] {
  const trimmed = query.trim().toLowerCase();
  const sessions = [...chat.sessions].filter((session) => !bots.forSession(session.session_id) && !isProviderConversationRuntime(getSessionAgentRuntime(session.session_id)))
    .map((session) => {
      const label = formatSessionLabel(session);
      const haystack = `${label} ${session.session_id} ${session.preview}`.toLowerCase();
      const score = trimmed ? fuzzyScore(trimmed, haystack) : session === chat.sessions[0] ? 80 : 40;
      return { session, label, score };
    })
    .filter((row) => !trimmed || row.score > 0)
    .sort(
      (a, b) =>
        b.score - a.score ||
        (b.session.last_timestamp ?? "").localeCompare(a.session.last_timestamp ?? ""),
    )
    .slice(0, limit);

  return sessions.map(({ session, label }) => ({
    id: `open-session:${session.session_id}`,
    section: "open" as const,
    label: `Open chat: ${label}`,
    subtitle: session.session_id.slice(0, 8),
    keywords: `session chat ${label} ${session.session_id}`,
    preview: {
      kind: "chat",
      sessionId: session.session_id,
      text: session.preview?.trim() || "Open this conversation.",
    },
    run: async (runCtx) => {
      await runCtx.chat.switchSession(session.session_id);
      runCtx.navigate("chat");
      runCtx.callbacks.focusChat();
      runCtx.callbacks.close();
    },
  }));
}

export function buildWorkCardOpenCommands(
  ctx: WorkshopCommandContext,
  query: string,
  limit = 8,
): WorkshopCommand[] {
  const trimmed = query.trim().toLowerCase();
  return workspace.cards
    .map((card) => {
      const haystack = `${card.title} ${card.status_label} ${columnLabel(card.column)}`.toLowerCase();
      const score = trimmed ? fuzzyScore(trimmed, haystack) : card.column === "blocked" ? 90 : 30;
      return { card, score };
    })
    .filter((row) => !trimmed || row.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map(({ card }) => ({
      id: `open-card:${card.id}`,
      section: "open" as const,
      label: `Open work: ${card.title}`,
      subtitle: columnLabel(card.column),
      keywords: `work card kanban ${card.title} ${card.id}`,
      run: async (runCtx) => {
        runCtx.workspace.workView = "hub";
        runCtx.navigate("work");
        await runCtx.workspace.selectCard(card.id);
        runCtx.callbacks.close();
      },
    }));
}

export function buildRecentSessionCommands(ctx: WorkshopCommandContext): WorkshopCommand[] {
  return buildSessionOpenCommands(ctx, "", 3);
}

export function buildBrowserHistoryCommands(
  query: string,
  limit = 8,
): WorkshopCommand[] {
  const trimmed = query.trim().toLowerCase();
  const entries = trimmed
    ? browserHistory.search(query, limit)
    : browserHistory.recent(limit);

  return entries.map((entry) => ({
    id: `browser-history:${entry.url}:${entry.visitedAt}`,
    section: "open" as const,
    label: entry.title || entry.url,
    subtitle: entry.url,
    keywords: `browser history web ${entry.title} ${entry.url}`,
    run: async (ctx) => {
      ctx.navigate("web");
      await humanBrowser.navigate(entry.url);
      ctx.callbacks.close();
    },
  }));
}


export function buildBotOpenCommands(query: string, limit = 12): WorkshopCommand[] {
  const needle = query.trim().toLowerCase();
  return bots.bots.filter((bot) => !bot.archived)
    .map((bot) => ({bot,score:fuzzyScore(needle,`bot ${bot.display_name} ${bot.role_description ?? ""} ${bot.primary_manuscript_id} ${bot.external_agent?.runtime ?? "medousa"}`.toLowerCase())}))
    .filter((row) => row.score > 0).sort((a,b) => b.score-a.score).slice(0,limit)
    .map(({bot}) => ({
      id:`open-bot:${bot.bot_id}`, section:"bots", label:bot.display_name,
      subtitle:bot.external_agent ? `${agentRuntimeLabel(bot.external_agent.runtime)} Bot` : "Bot",
      keywords:`bot ${bot.display_name} ${bot.role_description ?? ""} ${bot.primary_manuscript_id} ${bot.external_agent?.runtime ?? "medousa"}`,
      preview:{kind:"agent",name:bot.display_name,avatarRef:bot.avatar_ref,description:bot.role_description ?? "Open this Bot’s conversation."},
      run:async(ctx) => {
        const scope = ctx.chat.workshopScopeId;
        const response = await bots.open(bot);
        if (scope !== ctx.chat.workshopScopeId) return;
        await ctx.chat.switchSession(response.binding.session_id);
        shellTabs.openChat(response.binding.session_id,{title:response.bot.display_name,activate:true});
        ctx.callbacks.focusChat();ctx.callbacks.close();
      },
    }));
}
export function buildConnectedAgentOpenCommands(query: string, limit = 12): WorkshopCommand[] {
  if (connectedAgents.workshopScopeId !== chat.workshopScopeId) return [];
  const needle = query.trim().toLowerCase();
  return connectedAgents.conversations.map((agent) => ({agent,score:fuzzyScore(needle,`agent ${agent.label} ${agentRuntimeLabel(agent.provider)}`.toLowerCase())}))
    .filter((row) => row.score > 0).sort((a,b) => b.score-a.score).slice(0,limit)
    .map(({agent}) => ({
      id:`open-agent:${agent.provider}:${agent.id}`,section:"agents",label:agent.label,
      subtitle:agentRuntimeLabel(agent.provider),keywords:`agent ${agent.label} ${agentRuntimeLabel(agent.provider)}`,
      preview:{kind:"agent",name:agent.label,description:`${agentRuntimeLabel(agent.provider)} conversation attached to this workshop.`},
      run:async(ctx) => {
        const scope = ctx.chat.workshopScopeId;
        const id = externalConversationSessionId(agent.provider,agent.id);
        await ctx.chat.switchSession(id);if (scope !== ctx.chat.workshopScopeId) return; shellTabs.openChat(id,{title:agent.label,activate:true});
        ctx.callbacks.focusChat();ctx.callbacks.close();
      },
    }));
}
