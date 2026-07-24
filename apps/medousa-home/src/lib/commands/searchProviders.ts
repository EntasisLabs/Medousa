import { chat } from "$lib/stores/chat.svelte";
import { browserHistory } from "$lib/stores/browserHistory.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
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
        runCtx.navigate("library");
        await runCtx.vault.openNote(note.path);
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
  const sessions = [...chat.sessions]
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
