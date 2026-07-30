/** Shared transitions into an undertaking's permanent workspace surfaces. */

import { beginHumanAttempt, humanizeForgeMessage, type ItemProjection } from "$lib/forge";
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
import { getUndertakingSourceTreeShared } from "$lib/utils/forgeSourceTreeCache";

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

const LANDING_CANDIDATES = [
  "README.md",
  "README",
  "readme.md",
  "src/main.ts",
  "src/main.rs",
  "src/lib.rs",
  "src/index.ts",
  "src/index.js",
  "main.go",
  "Cargo.toml",
  "package.json",
];

export type LandCodeResult =
  | { ok: true; path: string }
  | { ok: false; error: string };

/** Open a real buffer after Start/open so Code never lands on an empty plaza. */
export async function landCodeWorkingSet(workId: string): Promise<LandCodeResult> {
  const id = workId.trim();
  if (!id) {
    return { ok: false, error: "No project selected." };
  }

  const detail = undertakings.detail?.id === id ? undertakings.detail : null;
  if (detail && !detail.environment) {
    const message = detail.allowed_actions.provision.allowed
      ? "Set up this project to open its working copy and files."
      : detail.allowed_actions.provision.reason ||
        "This project has no working copy yet.";
    codeWorkspace.workspaceErrorByWorkId = {
      ...codeWorkspace.workspaceErrorByWorkId,
      [id]: message,
    };
    return { ok: false, error: message };
  }

  try {
    await codeWorkspace.hydrate(id);
    const existing = codeWorkspace.activeFor(id);
    if (existing && !existing.loading && existing.digest) {
      undertakings.setSelection({ path: existing.path, line: existing.line ?? 1, entityId: null });
      codeWorkspace.workspaceErrorByWorkId = {
        ...codeWorkspace.workspaceErrorByWorkId,
        [id]: null,
      };
      return { ok: true, path: existing.path };
    }
    const tree = await getUndertakingSourceTreeShared(id);
    const paths = tree.files.map((file) => file.path);
    if (paths.length === 0) {
      const message = "This working copy has no files to open yet.";
      codeWorkspace.workspaceErrorByWorkId = {
        ...codeWorkspace.workspaceErrorByWorkId,
        [id]: message,
      };
      return { ok: false, error: message };
    }
    const preferred =
      LANDING_CANDIDATES.find((candidate) => paths.includes(candidate)) ??
      paths.find((path) => /\.(ts|tsx|js|jsx|rs|go|py|svelte|md)$/i.test(path)) ??
      paths[0];
    if (!preferred) {
      const message = "Could not pick a landing file in this working copy.";
      codeWorkspace.workspaceErrorByWorkId = {
        ...codeWorkspace.workspaceErrorByWorkId,
        [id]: message,
      };
      return { ok: false, error: message };
    }
    await codeWorkspace.open(id, preferred, 1);
    undertakings.setSelection({ path: preferred, line: 1, entityId: null });
    codeWorkspace.workspaceErrorByWorkId = {
      ...codeWorkspace.workspaceErrorByWorkId,
      [id]: null,
    };
    return { ok: true, path: preferred };
  } catch (err) {
    const message = humanizeForgeMessage(
      err instanceof Error ? err.message : String(err),
    );
    codeWorkspace.workspaceErrorByWorkId = {
      ...codeWorkspace.workspaceErrorByWorkId,
      [id]: message,
    };
    return { ok: false, error: message };
  }
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

export async function openTrackedTerminal(
  item: ItemProjection,
  options?: { activate?: boolean },
): Promise<string | null> {
  if (undertakings.active?.workId !== item.id) undertakings.setActiveFromItem(item);

  const existing =
    undertakings.active?.workId === item.id
      ? undertakings.active.boundTerminalSessionIds[0]
      : null;
  if (existing) {
    shellTabs.openTerminal(existing, {
      activate: options?.activate !== false,
      title: `Terminal · ${item.title}`,
      workId: item.id,
    });
    return existing;
  }

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
    activate: options?.activate !== false,
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

/** Stop the bound agent process without taking the editing lease. */
export async function interruptTrackedAgent(item: ItemProjection): Promise<void> {
  const active = undertakings.active;
  if (active?.workId !== item.id) return;
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
  undertakings.setActiveFromItem(item, { executorKind: "human" });
}

export async function reclaimTrackedHuman(item: ItemProjection): Promise<ItemProjection> {
  await interruptTrackedAgent(item);
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
