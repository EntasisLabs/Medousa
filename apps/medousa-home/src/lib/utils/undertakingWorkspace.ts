/** Shared transitions into an undertaking's permanent workspace surfaces. */

import { beginHumanAttempt, type ItemProjection } from "$lib/forge";
import { terminalCreate } from "$lib/terminal";
import { undertakings } from "$lib/stores/undertakings.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { chat } from "$lib/stores/chat.svelte";
import {
  cancelAgentSession,
  createAgentSession,
  type CodeIntentContext,
} from "$lib/daemon";
import {
  clearSessionAgentSessionId,
  getSessionAgentSessionId,
  setSessionAgentRuntime,
  setSessionAgentSessionId,
} from "$lib/utils/sessionAgentRuntime";
import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";

function terminalSessionId(created: { session_id?: string; id?: string }): string {
  return typeof created.session_id === "string"
    ? created.session_id
    : String(created.id ?? "");
}

type ActiveCodeInsights = Pick<
  CodeIntentContext,
  "containing_symbol" | "diagnostics" | "last_verification"
>;

const codeInsightsByWorkId = new Map<string, ActiveCodeInsights>();

export function setActiveCodeInsights(workId: string, insights: ActiveCodeInsights) {
  if (!workId.trim()) return;
  codeInsightsByWorkId.set(workId, insights);
}

export function activeCodeContext(sessionId: string): CodeIntentContext | null {
  const active = undertakings.active;
  if (!active || !active.boundChatSessionIds.includes(sessionId)) return null;
  const detail = undertakings.detail?.id === active.workId ? undertakings.detail : null;
  const openFiles = codeWorkspace.tabsFor(active.workId).map((tab) => tab.path).slice(0, 12);
  const insights = codeInsightsByWorkId.get(active.workId);
  return {
    work_id: active.workId,
    project_title: active.title,
    outcome: detail?.brief ?? null,
    active_path: active.selectedPath,
    cursor_line: active.selectedLine,
    selection_start_line: active.selectionStartLine,
    selection_end_line: active.selectionEndLine,
    selected_text: active.selectedText,
    open_files: openFiles,
    containing_symbol: insights?.containing_symbol,
    diagnostics: insights?.diagnostics,
    last_verification: insights?.last_verification,
  };
}

export async function openTrackedTerminal(item: ItemProjection): Promise<string | null> {
  if (undertakings.active?.workId !== item.id) undertakings.setActiveFromItem(item);

  let leaseId = undertakings.active?.leaseId ?? null;
  if (item.allowed_actions.begin_attempt.allowed) {
    const begun = await beginHumanAttempt(item.id);
    leaseId = begun.lease.lease_id;
    undertakings.setActiveFromItem(begun.item, {
      leaseId,
      leaseGeneration: begun.lease.generation,
      executorKind: "human",
    });
  }

  const created = await terminalCreate({ work_id: item.id, lease_id: leaseId });
  const sessionId = terminalSessionId(created);
  if (!sessionId) return null;

  undertakings.bindTerminal(sessionId);
  shellTabs.openTerminal(sessionId, {
    activate: true,
    title: `Terminal · ${item.title}`,
    workId: item.id,
  });
  return sessionId;
}

export async function startTrackedAgent(
  item: ItemProjection,
  runtime: "codex" | "cursor",
  options?: { draft?: string },
): Promise<string> {
  const currentSession = chat.sessionId;
  const canReuseCurrentChat =
    undertakings.active?.workId === item.id &&
    !!currentSession &&
    undertakings.active.boundChatSessionIds.includes(currentSession);
  if (!canReuseCurrentChat) await chat.newSession();
  const sessionId = chat.sessionId;
  if (!sessionId) throw new Error("Could not create a Chat workspace for this undertaking");

  const accepted = await createAgentSession({
    session_id: sessionId,
    runtime,
    work_id: item.id,
  });
  setSessionAgentRuntime(sessionId, runtime);
  setSessionAgentSessionId(sessionId, accepted.agent_session_id);
  undertakings.setActiveFromItem(item, { executorKind: runtime });
  undertakings.bindChat(sessionId);
  shellTabs.openChat(sessionId, { activate: true });
  if (options?.draft?.trim()) {
    chat.prefillDraft(options.draft.trim());
    window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
  }
  return accepted.agent_session_id;
}

export async function reclaimTrackedHuman(item: ItemProjection): Promise<ItemProjection> {
  const active = undertakings.active;
  if (active?.workId === item.id) {
    for (const sessionId of [...active.boundChatSessionIds].reverse()) {
      const agentSessionId = getSessionAgentSessionId(sessionId);
      if (!agentSessionId) continue;
      try {
        await cancelAgentSession(agentSessionId);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        if (!/unknown agent session|not found|404/i.test(message)) throw err;
      }
      clearSessionAgentSessionId(sessionId);
      setSessionAgentRuntime(sessionId, "medousa");
      break;
    }
  }
  await undertakings.refreshDetail();
  const ready = undertakings.detail?.id === item.id ? undertakings.detail : item;
  const begun = await beginHumanAttempt(ready.id);
  undertakings.setActiveFromItem(begun.item, {
    leaseId: begun.lease.lease_id,
    leaseGeneration: begun.lease.generation,
    executorKind: "human",
  });
  return begun.item;
}
