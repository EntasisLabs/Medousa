/**
 * Per-session agent runtime preference (Medousa native vs ACP external).
 *
 * External runtimes (Cursor / Codex) use the daemon `/v1/agents` SDK façade.
 * Stasis 0.8 can also park `workflow.stasis.agent_turn.waitable` jobs on the
 * process-local TurnWaitStore until ACP completion feeds AgentEventIngress.
 * Home chat still selects runtimes here; it does not speak ACP or Stasis wait
 * stores directly.
 */

import { operationPath } from "$lib/daemon/opPath";

const STORAGE_KEY = "medousa-home-agent-runtime-v1";
const AGENT_SESSION_KEY = "medousa-home-agent-session-v1";
const AGENT_CONFIG_KEY = "medousa-home-agent-config-v1";
const AGENT_WORK_KEY = "medousa-home-agent-work-v1";

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

function loadAgentSessionMap(): Record<string, string> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(AGENT_SESSION_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, string>;
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(parsed)) {
      if (typeof v === "string" && v.trim()) out[k] = v.trim();
    }
    return out;
  } catch {
    return {};
  }
}

function saveAgentSessionMap(map: Record<string, string>) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(AGENT_SESSION_KEY, JSON.stringify(map));
}

function loadAgentWorkMap(): Record<string, string | null> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(AGENT_WORK_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const out: Record<string, string | null> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (value === null) out[key] = null;
      else if (typeof value === "string" && value.trim()) out[key] = value.trim();
    }
    return out;
  } catch {
    return {};
  }
}

function saveAgentWorkMap(map: Record<string, string | null>) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(AGENT_WORK_KEY, JSON.stringify(map));
}

export function getSessionAgentRuntime(sessionId: string): ChatAgentRuntime {
  const trimmed = sessionId.trim();
  if (!trimmed) return "medousa";
  return loadMap()[trimmed] ?? "medousa";
}

/** Active `/v1/agents` session for this chat (create once, prompt thereafter). */
export function getSessionAgentSessionId(sessionId: string): string | null {
  const trimmed = sessionId.trim();
  if (!trimmed) return null;
  return loadAgentSessionMap()[trimmed] ?? null;
}

export function setSessionAgentSessionId(
  sessionId: string,
  agentSessionId: string | null,
) {
  const trimmed = sessionId.trim();
  if (!trimmed) return;
  const map = loadAgentSessionMap();
  if (!agentSessionId?.trim()) {
    delete map[trimmed];
  } else {
    map[trimmed] = agentSessionId.trim();
  }
  saveAgentSessionMap(map);
}

/** Forge work item used when the current ACP process was created; `null` means plain chat. */
export function getSessionAgentWorkId(sessionId: string): string | null | undefined {
  const trimmed = sessionId.trim();
  if (!trimmed) return undefined;
  return loadAgentWorkMap()[trimmed];
}

export function setSessionAgentWorkId(sessionId: string, workId: string | null) {
  const trimmed = sessionId.trim();
  if (!trimmed) return;
  const map = loadAgentWorkMap();
  map[trimmed] = workId?.trim() || null;
  saveAgentWorkMap(map);
}

export function clearSessionAgentWorkId(sessionId: string) {
  const trimmed = sessionId.trim();
  if (!trimmed) return;
  const map = loadAgentWorkMap();
  delete map[trimmed];
  saveAgentWorkMap(map);
}

export function clearSessionAgentSessionId(sessionId: string) {
  setSessionAgentSessionId(sessionId, null);
  clearSessionAgentWorkId(sessionId);
}

export function getSessionAgentConfigOptions(sessionId: string): unknown[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const all = JSON.parse(localStorage.getItem(AGENT_CONFIG_KEY) ?? "{}") as Record<
      string,
      unknown
    >;
    const value = all[sessionId.trim()];
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}

export function setSessionAgentConfigOptions(sessionId: string, options: unknown[]) {
  if (typeof localStorage === "undefined" || !sessionId.trim()) return;
  try {
    const all = JSON.parse(localStorage.getItem(AGENT_CONFIG_KEY) ?? "{}") as Record<
      string,
      unknown
    >;
    if (options.length > 0) all[sessionId.trim()] = options;
    else delete all[sessionId.trim()];
    localStorage.setItem(AGENT_CONFIG_KEY, JSON.stringify(all));
  } catch {
    // Storage is a convenience cache; the daemon remains authoritative.
  }
}

export function setSessionAgentRuntime(
  sessionId: string,
  runtime: ChatAgentRuntime,
) {
  const trimmed = sessionId.trim();
  if (!trimmed) return;
  const map = loadMap();
  const previous = map[trimmed] ?? "medousa";
  if (runtime === "medousa") {
    delete map[trimmed];
  } else {
    map[trimmed] = runtime;
  }
  saveMap(map);
  // Switching runtime (including back to Medousa) drops the ACP session id.
  if (previous !== runtime) {
    clearSessionAgentSessionId(trimmed);
    setSessionAgentConfigOptions(trimmed, []);
  }
}

export function agentRuntimeLabel(runtime: ChatAgentRuntime): string {
  switch (runtime) {
    case "cursor":
      return "Cursor";
    case "codex":
      return "ChatGPT / Codex";
    default:
      return "Medousa";
  }
}

export function agentSessionStreamUrl(agentSessionId: string): string {
  return operationPath("agents.sessions.by_agent_session_id.stream.get", {
    agent_session_id: agentSessionId.trim(),
  });
}
