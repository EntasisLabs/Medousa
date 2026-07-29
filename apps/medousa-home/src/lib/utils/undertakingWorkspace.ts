/** Shared transitions into an undertaking's permanent workspace surfaces. */

import { beginHumanAttempt, type ItemProjection } from "$lib/forge";
import { terminalCreate } from "$lib/terminal";
import { undertakings } from "$lib/stores/undertakings.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { createAgentSession } from "$lib/daemon";
import { setSessionAgentSessionId } from "$lib/utils/sessionAgentRuntime";

function terminalSessionId(created: { session_id?: string; id?: string }): string {
  return typeof created.session_id === "string"
    ? created.session_id
    : String(created.id ?? "");
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
  setSessionAgentSessionId(sessionId, accepted.agent_session_id);
  undertakings.setActiveFromItem(item, { executorKind: runtime });
  undertakings.bindChat(sessionId);
  shellTabs.openChat(sessionId, { activate: true });
  return accepted.agent_session_id;
}
