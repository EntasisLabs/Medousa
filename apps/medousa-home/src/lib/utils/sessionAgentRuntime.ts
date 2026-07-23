/**
 * Per-session agent runtime preference (Medousa native vs ACP external).
 *
 * External runtimes (Cursor / Codex) use the daemon `/v1/agents` SDK façade.
 * Stasis 0.8 can also park `workflow.stasis.agent_turn.waitable` jobs on the
 * process-local TurnWaitStore until ACP completion feeds AgentEventIngress.
 * Home chat still selects runtimes here; it does not speak ACP or Stasis wait
 * stores directly.
 */

const STORAGE_KEY = "medousa-home-agent-runtime-v1";

export type ChatAgentRuntime = "medousa" | "cursor" | "codex";

const VALID = new Set<ChatAgentRuntime>(["medousa", "cursor", "codex"]);

/** Cursor/Codex — external ACP participants (waitable turns on the daemon). */
export function isExternalAgentRuntime(runtime: ChatAgentRuntime): boolean {
  return runtime === "cursor" || runtime === "codex";
}

function loadMap(): Record<string, ChatAgentRuntime> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, string>;
    const out: Record<string, ChatAgentRuntime> = {};
    for (const [k, v] of Object.entries(parsed)) {
      if (VALID.has(v as ChatAgentRuntime)) out[k] = v as ChatAgentRuntime;
    }
    return out;
  } catch {
    return {};
  }
}

function saveMap(map: Record<string, ChatAgentRuntime>) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
}

export function getSessionAgentRuntime(sessionId: string): ChatAgentRuntime {
  const trimmed = sessionId.trim();
  if (!trimmed) return "medousa";
  return loadMap()[trimmed] ?? "medousa";
}

export function setSessionAgentRuntime(
  sessionId: string,
  runtime: ChatAgentRuntime,
) {
  const trimmed = sessionId.trim();
  if (!trimmed) return;
  const map = loadMap();
  if (runtime === "medousa") {
    delete map[trimmed];
  } else {
    map[trimmed] = runtime;
  }
  saveMap(map);
}

export function agentRuntimeLabel(runtime: ChatAgentRuntime): string {
  switch (runtime) {
    case "cursor":
      return "Cursor";
    case "codex":
      return "Codex";
    default:
      return "Medousa";
  }
}
