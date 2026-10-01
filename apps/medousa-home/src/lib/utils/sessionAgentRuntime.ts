/**
 * Per-session agent runtime preference (Medousa native vs ACP external).
 *
 * External runtimes (Cursor / Codex / Hermes) use the daemon agents SDK façade.
 * Stasis 0.8 can also park `workflow.stasis.agent_turn.waitable` jobs on the
 * process-local TurnWaitStore until ACP completion feeds AgentEventIngress.
 * Home chat still selects runtimes here; it does not speak ACP or Stasis wait
 * stores directly.
 */

import { operationPath } from "$lib/daemon/opPath";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";

import { externalConversationBinding } from "$lib/utils/externalConversationSession";

const STORAGE_KEY = "medousa-home-agent-runtime-v1";
const AGENT_SESSION_KEY = "medousa-home-agent-session-v1";
const AGENT_CONFIG_KEY = "medousa-home-agent-config-v1";
const AGENT_WORK_KEY = "medousa-home-agent-work-v1";
const AGENT_CONTEXT_KEY = "medousa-home-agent-context-v1";

function scopedKey(key: string): string {
  return workshopScopedStorageKey(key);
}

export type ChatAgentRuntime = "medousa" | "cursor" | "codex" | "hermes" | "muse" | "grok_bot" | "instinct" | "dots";

const VALID = new Set<ChatAgentRuntime>(["medousa", "cursor", "codex", "hermes", "muse", "grok_bot", "instinct", "dots"]);

/** Cursor/Codex/Hermes — external ACP participants (waitable turns on the daemon). */
export function isExternalAgentRuntime(runtime: ChatAgentRuntime): runtime is "cursor" | "codex" | "hermes" {
  return runtime === "cursor" || runtime === "codex" || runtime === "hermes";
}

export function isProviderConversationRuntime(runtime: ChatAgentRuntime): runtime is "muse" | "grok_bot" | "instinct" | "dots" {
  return runtime === "muse" || runtime === "grok_bot" || runtime === "instinct" || runtime === "dots";
}

function loadMap(): Record<string, ChatAgentRuntime> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(scopedKey(STORAGE_KEY));
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
  localStorage.setItem(scopedKey(STORAGE_KEY), JSON.stringify(map));
}

function loadAgentSessionMap(): Record<string, string> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(scopedKey(AGENT_SESSION_KEY));
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
  localStorage.setItem(scopedKey(AGENT_SESSION_KEY), JSON.stringify(map));
}

function loadAgentWorkMap(): Record<string, string | null> {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(scopedKey(AGENT_WORK_KEY));
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
  localStorage.setItem(scopedKey(AGENT_WORK_KEY), JSON.stringify(map));
}

export function getSessionAgentRuntime(sessionId: string): ChatAgentRuntime {
  const trimmed = sessionId.trim();
  if (!trimmed) return "medousa";
  return externalConversationBinding(trimmed)?.provider ?? loadMap()[trimmed] ?? "medousa";
}

/** Active agents session for this chat (create once, prompt thereafter). */
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

export function agentConversationContextSeeded(sessionId: string, agentSessionId: string): boolean {
  if (typeof localStorage === "undefined") return false;
  try {
    return JSON.parse(localStorage.getItem(scopedKey(AGENT_CONTEXT_KEY)) ?? "{}")[sessionId.trim()] === agentSessionId;
  } catch { return false; }
}

export function markAgentConversationContextSeeded(sessionId: string, agentSessionId: string) {
  if (typeof localStorage === "undefined") return;
  try {
    const map = JSON.parse(localStorage.getItem(scopedKey(AGENT_CONTEXT_KEY)) ?? "{}");
    map[sessionId.trim()] = agentSessionId;
    localStorage.setItem(scopedKey(AGENT_CONTEXT_KEY), JSON.stringify(map));
  } catch { /* A cache failure only means the next prompt may repeat context. */ }
}

export function getSessionAgentConfigOptions(sessionId: string): unknown[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const all = JSON.parse(localStorage.getItem(scopedKey(AGENT_CONFIG_KEY)) ?? "{}") as Record<
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
    const all = JSON.parse(localStorage.getItem(scopedKey(AGENT_CONFIG_KEY)) ?? "{}") as Record<
      string,
      unknown
    >;
    if (options.length > 0) all[sessionId.trim()] = options;
    else delete all[sessionId.trim()];
    localStorage.setItem(scopedKey(AGENT_CONFIG_KEY), JSON.stringify(all));
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
  if (externalConversationBinding(trimmed)) return;
  const map = loadMap();
  const previous = map[trimmed] ?? "medousa";
  if (isProviderConversationRuntime(previous) && previous !== runtime) return;
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
    case "hermes":
      return "Hermes";
    case "instinct":
      return "Instinct Agent";
    case "dots":
      return "Dots";
    case "muse":
      return "Muse";
    case "grok_bot":
      return "Grok Bot";
    default:
      return "Medousa";
  }
}

export function agentSessionStreamUrl(agentSessionId: string): string {
  return operationPath("agents.sessions.by_agent_session_id.stream.get", {
    agent_session_id: agentSessionId.trim(),
  });
}
