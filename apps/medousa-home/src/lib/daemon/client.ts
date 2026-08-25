import { invoke } from "@tauri-apps/api/core";
import type { DaemonRuntimeDescriptor } from "$lib/types/generated/daemon_api";

export type { DaemonRuntimeDescriptor };

export interface DaemonHealth {
  ok: boolean;
  message: string;
  runtime?: DaemonRuntimeDescriptor | null;
  backend?: string | null;
  worker_id?: string | null;
  tool_registry_count?: number | null;
  agent_runtime_version?: string | null;
  last_agent_turn_at_utc?: string | null;
  last_agent_turn_latency_ms?: number | null;
  active_profile_id?: string | null;
  active_profile_display_name?: string | null;
}

export async function getDaemonUrl(): Promise<string> {
  return invoke<string>("daemon_url");
}

export async function setDaemonUrl(url: string): Promise<void> {
  return invoke("set_daemon_url", { url });
}

/**
 * Flush both transport route caches (JSON/health + SSE/multipart) so a network
 * handoff that happened while backgrounded forces a fresh LAN-vs-Iroh probe.
 */
export async function invalidateRouteCaches(): Promise<void> {
  return invoke("invalidate_route_caches");
}

/** Plain JSON clone — strips Svelte proxies before Tauri IPC serialization. */
export function invokePlain<T>(value: T): T {
  if (value === null || value === undefined) return value;
  return JSON.parse(JSON.stringify(value)) as T;
}

export type StreamErrorPayload = {
  message: string;
  recoverable?: boolean;
  transport?: string;
  stage?: string;
};

export function daemonWebSocketUrl(path: string): Promise<string> {
  return getDaemonUrl().then((base) => {
    const normalized = base.replace(/\/$/, "");
    const wsBase = normalized.replace(/^http/i, "ws");
    return `${wsBase}${path.startsWith("/") ? path : `/${path}`}`;
  });
}
