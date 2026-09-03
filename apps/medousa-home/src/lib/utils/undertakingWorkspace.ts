/** Shared transitions into an undertaking's permanent workspace surfaces. */

import {
  canStartHumanEditing,
  discardUndertaking,
  prepareExecutorHandoff,
  startHumanEditingSession,
  humanizeForgeMessage,
  usesAttachedCheckout,
  type ItemProjection,
} from "$lib/forge";
import { terminalCreate } from "$lib/terminal";
import { undertakings } from "$lib/stores/undertakings.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { chat } from "$lib/stores/chat.svelte";
import {
  cancelActiveSessionTurn,
  cancelAgentSession,
  setSessionCodeBinding,
  createAgentSession,
  type CodeIntentContext,
} from "$lib/daemon";
import {
  clearSessionAgentSessionId,
  getSessionAgentSessionId,
  setSessionAgentRuntime,
  setSessionAgentSessionId,
  setSessionAgentWorkId,
} from "$lib/utils/sessionAgentRuntime";
import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
import { landCodeWorkingSet as landCodeWorkingSetThroughController } from "$lib/utils/codeWorkspaceController";
import type { LandCodeResult } from "$lib/utils/codeWorkspaceController";

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
export type { LandCodeResult } from "$lib/utils/codeWorkspaceController";

export function undertakingWorkspaceCopy(item: ItemProjection | null) {
  const attached = usesAttachedCheckout(item);
  return {
    attached,
    label: attached ? "current checkout" : "working copy",
    finishPrompt: attached
      ? "Finish this project and keep the changes in this checkout?"
      : "Finish this project and keep its branch?",
    closePrompt: attached
      ? `Close “${item?.title ?? "this project"}”? Files in the current checkout will stay exactly where they are.`
      : `Close “${item?.title ?? "this project"}”? Its working copy will be removed.`,
    setupGuidance: attached
      ? "Attach the current checkout so the tree and editor can open."
      : "Create the working copy so the tree and editor can open.",
  };
}

/** Open a real buffer after Start/open so Code never lands on an empty plaza. */
export async function landCodeWorkingSet(workId: string): Promise<LandCodeResult> {
  return landCodeWorkingSetThroughController(workId);
}

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
  const revisionBrief = undertakings.review?.work_id === active.workId
    ? undertakings.review.revision_brief?.trim() || null
    : null;
  return {
    work_id: active.workId,
    project_title: active.title,
    outcome: revisionBrief || detail?.brief || null,
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

export async function openTrackedTerminal(
  item: ItemProjection,
  options?: { activate?: boolean },
): Promise<string | null> {
  if (undertakings.active?.workId !== item.id) undertakings.setActiveFromItem(item);

  const existing =
    undertakings.active?.workId === item.id
      ? undertakings.active.boundTerminalSessionIds[0]
      : null;

  const openShellTab = options?.activate !== false;

  if (existing) {
    if (openShellTab) {
      shellTabs.openTerminal(existing, {
        activate: true,
        title: `Terminal · ${item.title}`,
        workId: item.id,
      });
    }
    return existing;
  }

  let leaseId = undertakings.active?.leaseId ?? null;
  if (canStartHumanEditing(item.allowed_actions)) {
    const begun = await startHumanEditingSession(item.id, item.allowed_actions);
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
  if (openShellTab) {
    shellTabs.openTerminal(sessionId, {
      activate: true,
      title: `Terminal · ${item.title}`,
      workId: item.id,
    });
  }
  return sessionId;
}

export async function startTrackedAgent(
  item: ItemProjection,
  runtime: "codex" | "cursor" | "hermes",
  options?: { draft?: string },
): Promise<string> {
  let readyItem = item;
  if (usesAttachedCheckout(item)) {
    const activeAttemptIds = new Set(item.active_attempts ?? []);
    if (item.active_attempt) activeAttemptIds.add(item.active_attempt);
    const humanAttempt = (item.attempts ?? []).find(
      (attempt) =>
        activeAttemptIds.has(attempt.id) &&
        attempt.executor?.kind === "human" &&
        attempt.lease,
    );
    if (humanAttempt?.lease) {
      readyItem = await prepareExecutorHandoff({
        work_id: item.id,
        lease_id: humanAttempt.lease.lease_id,
        generation: humanAttempt.lease.generation,
        to_executor: runtime,
      });
      undertakings.setActiveFromItem(readyItem);
    }
  }

  const currentSession = chat.sessionId;
  const canReuseCurrentChat =
    undertakings.active?.workId === item.id &&
    !!currentSession &&
    undertakings.active.boundChatSessionIds.includes(currentSession) &&
    !getSessionAgentSessionId(currentSession);
  if (!canReuseCurrentChat) await chat.newSession();
  const sessionId = chat.sessionId;
  if (!sessionId) throw new Error("Could not create a Chat workspace for this undertaking");

  const accepted = await createAgentSession({
    session_id: sessionId,
    runtime,
    work_id: readyItem.id,
  });
  setSessionAgentRuntime(sessionId, runtime);
  setSessionAgentSessionId(sessionId, accepted.agent_session_id);
  setSessionAgentWorkId(sessionId, readyItem.id);
  undertakings.setActiveFromItem(readyItem, { executorKind: runtime });
  undertakings.bindChat(sessionId);
  await setSessionCodeBinding(sessionId, readyItem.id);
  shellTabs.openChat(sessionId, { activate: true });
  if (options?.draft?.trim()) {
    chat.prefillDraft(options.draft.trim());
    window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
  }
  return accepted.agent_session_id;
}

/** Stop the bound agent process without taking the editing lease. */
export async function interruptTrackedAgent(item: ItemProjection): Promise<void> {
  const active = undertakings.active;
  const boundChatSessionIds = active?.workId === item.id ? active.boundChatSessionIds : [];
  const knownChatSessionIds = new Set(boundChatSessionIds);
  const activeAttemptIds = new Set(item.active_attempts ?? []);
  if (item.active_attempt) activeAttemptIds.add(item.active_attempt);
  const activeAttempts = (item.attempts ?? []).filter((attempt) =>
    activeAttemptIds.has(attempt.id),
  );
  const agentSessionIds = new Set<string>();
  const nativeSessionIds = new Set<string>();

  for (const attempt of activeAttempts) {
    const detail = attempt.executor?.detail;
    const agentSessionId = detail?.agent_session_id;
    if (typeof agentSessionId === "string" && agentSessionId.trim()) {
      agentSessionIds.add(agentSessionId.trim());
    }
    const chatSessionId = detail?.chat_session_id;
    if (typeof chatSessionId === "string" && chatSessionId.trim()) {
      knownChatSessionIds.add(chatSessionId.trim());
    }
    if (attempt.executor?.kind === "medousa-coder") {
      const sessionId = detail?.session_id;
      if (typeof sessionId === "string" && sessionId.trim()) {
        nativeSessionIds.add(sessionId.trim());
        knownChatSessionIds.add(sessionId.trim());
      }
    }
  }

  for (const sessionId of [...boundChatSessionIds].reverse()) {
    const agentSessionId = getSessionAgentSessionId(sessionId);
    if (agentSessionId) agentSessionIds.add(agentSessionId);
  }

  for (const agentSessionId of agentSessionIds) {
    try {
      await cancelAgentSession(agentSessionId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (!/unknown agent session|not found|404/i.test(message)) throw err;
    }
  }
  for (const sessionId of nativeSessionIds) {
    await cancelActiveSessionTurn(sessionId);
  }
  for (const sessionId of knownChatSessionIds) {
    if (!getSessionAgentSessionId(sessionId)) continue;
    clearSessionAgentSessionId(sessionId);
    setSessionAgentRuntime(sessionId, "medousa");
  }
  if (active?.workId === item.id) {
    undertakings.setActiveFromItem(item, { executorKind: "human" });
  }
}

/**
 * Close an undertaking: cancel any bound agent, then release its Forge
 * workspace. Isolated copies are reclaimed; attached checkout files stay put.
 */
export async function closeUndertaking(item: ItemProjection): Promise<void> {
  if (usesAttachedCheckout(item)) {
    await interruptTrackedAgent(item);
  } else {
    try {
      await interruptTrackedAgent(item);
    } catch {
      // Removing an isolated copy revokes access even if agent cancel failed.
    }
  }
  await discardUndertaking(item.id);
}

export async function reclaimTrackedHuman(item: ItemProjection): Promise<ItemProjection> {
  await interruptTrackedAgent(item);
  await undertakings.refreshDetail();
  const ready = undertakings.detail?.id === item.id ? undertakings.detail : item;
  const begun = await startHumanEditingSession(ready.id, ready.allowed_actions);
  undertakings.setActiveFromItem(begun.item, {
    leaseId: begun.lease.lease_id,
    leaseGeneration: begun.lease.generation,
    executorKind: "human",
  });
  return begun.item;
}
