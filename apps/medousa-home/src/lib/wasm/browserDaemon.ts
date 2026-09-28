import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { CreateSessionResponse } from "$lib/types/generated/daemon_api";
import type { SessionSummary } from "$lib/types/session";

interface BrowserDaemonExports {
  default?: (input?: unknown) => Promise<unknown>;
  boot: () => Promise<void>;
  configure_inference: (baseUrl: string, apiKey: string, model: string) => void;
  list_sessions: () => Promise<string>;
  create_session: (title: string) => Promise<string>;
  list_notes: () => Promise<string>;
  save_note: (noteId: string, title: string, body: string) => Promise<string>;
  start_turn: (
    sessionId: string,
    text: string,
    onEvent: (event: string) => void,
  ) => Promise<string>;
  run_grapheme: (source: string) => string;
  dial_iroh_ticket: (ticket: string, path: string) => Promise<string>;
  pair_from_invite: (qrUrl: string, displayName: string) => Promise<string>;
  set_active_portal: (workshopId: string) => void;
  forget_portal: (workshopId: string) => void;
  portal_request: (method: string, path: string, body: string) => Promise<string>;
  portal_open_stream: (
    kind: string,
    path: string,
    accept: string,
    onEvent: (data: string) => void,
    onError: (message: string) => void,
  ) => void;
  portal_stop_streams: (prefix: string) => void;
  read_vault: (path: string) => Promise<string>;
  write_vault: (path: string, body: string) => Promise<void>;
}

export interface BrowserTurnEvent {
  kind: "accepted" | "delta" | "done" | "error" | string;
  turn_id: string;
  text?: string;
  message?: string;
}

const listeners = new Set<(event: InteractiveTurnStreamEvent) => void>();
const buffered: InteractiveTurnStreamEvent[] = [];
let daemonPromise: Promise<BrowserDaemonExports> | null = null;

function publish(event: InteractiveTurnStreamEvent) {
  if (listeners.size === 0) {
    buffered.push(event);
    return;
  }
  for (const listener of listeners) listener(event);
}

export function subscribeBrowserTurnEvents(
  handler: (event: InteractiveTurnStreamEvent) => void,
): () => void {
  listeners.add(handler);
  const pending = buffered.splice(0);
  for (const event of pending) handler(event);
  return () => listeners.delete(handler);
}

export function mapBrowserTurnEvent(event: BrowserTurnEvent): InteractiveTurnStreamEvent {
  const now = new Date().toISOString();
  const turnId = event.turn_id;
  if (event.kind === "delta") {
    return {
      emitted_at_utc: now,
      event_type: "content_delta",
      message: "",
      phase: "streaming",
      terminal: false,
      turn_id: turnId,
      content_delta: event.text ?? "",
    };
  }
  if (event.kind === "done") {
    return {
      emitted_at_utc: now,
      event_type: "final",
      message: event.text ?? "",
      phase: "complete",
      terminal: true,
      turn_id: turnId,
      final_text: event.text ?? "",
    };
  }
  if (event.kind === "error") {
    return {
      emitted_at_utc: now,
      event_type: "error",
      message: event.message ?? "browser turn failed",
      phase: "error",
      terminal: true,
      turn_id: turnId,
    };
  }
  return {
    emitted_at_utc: now,
    event_type: "status",
    message: "",
    phase: "accepted",
    terminal: false,
    turn_id: turnId,
  };
}

export async function loadDaemon(): Promise<BrowserDaemonExports> {
  if (!daemonPromise) {
    daemonPromise = (async () => {
      const loadGlue = new Function(
        "specifier",
        "return import(specifier)",
      ) as (specifier: string) => Promise<BrowserDaemonExports>;
      const mod = await loadGlue("/wasm/medousa_browser.js");
      if (typeof mod.default === "function") await mod.default();
      await mod.boot();
      return mod;
    })().catch((error) => {
      daemonPromise = null;
      throw error;
    });
  }
  return daemonPromise;
}

export async function bootBrowserWorkshop(): Promise<void> {
  await loadDaemon();
}

export function configureBrowserInference(baseUrl: string, apiKey: string, model: string): Promise<void> {
  return loadDaemon().then((daemon) => {
    daemon.configure_inference(baseUrl, apiKey, model);
  });
}

export async function listBrowserSessions(limit?: number): Promise<{
  sessions: SessionSummary[];
  next_cursor: string | null;
}> {
  const daemon = await loadDaemon();
  const rows = JSON.parse(await daemon.list_sessions()) as Array<{
    session_id: string;
    title?: string;
    preview?: string;
    updated_ms?: number;
  }>;
  const sessions: SessionSummary[] = rows.slice(0, limit ?? rows.length).map((row) => ({
    session_id: row.session_id,
    display_name: row.title ?? null,
    turns: 0,
    verification_runs: 0,
    last_timestamp: row.updated_ms ? new Date(row.updated_ms).toISOString() : null,
    preview: row.preview ?? "",
  }));
  return { sessions, next_cursor: null };
}

export async function createBrowserSession(displayName?: string): Promise<CreateSessionResponse> {
  const daemon = await loadDaemon();
  const row = JSON.parse(await daemon.create_session(displayName ?? "")) as {
    session_id: string;
    title?: string;
    authority_id: string;
  };
  return {
    session_id: row.session_id,
    authority_id: row.authority_id,
    catalog: "single",
    display_name: row.title ?? displayName ?? null,
  };
}

export async function sendBrowserInteractiveTurn(
  sessionId: string,
  prompt: string,
): Promise<{ turn_id: string; stream_url: string }> {
  const daemon = await loadDaemon();
  const turnId = await new Promise<string>((resolve, reject) => {
    let settled = false;
    const finish = (id: string) => {
      if (settled || !id) return;
      settled = true;
      resolve(id);
    };
    daemon
      .start_turn(sessionId, prompt, (raw) => {
        const event = JSON.parse(raw) as BrowserTurnEvent;
        if (event.turn_id) finish(event.turn_id);
        publish(mapBrowserTurnEvent(event));
      })
      .then((id) => finish(id))
      .catch((error: unknown) => {
        if (!settled) reject(error instanceof Error ? error : new Error(String(error)));
      });
  });
  return { turn_id: turnId, stream_url: `browser:${turnId}` };
}

export async function runBrowserGrapheme(source: string): Promise<unknown> {
  const daemon = await loadDaemon();
  return JSON.parse(daemon.run_grapheme(source));
}

export async function dialBrowserIrohTicket(ticket: string, path = "/"): Promise<string> {
  const daemon = await loadDaemon();
  return daemon.dial_iroh_ticket(ticket, path);
}

export async function readBrowserVault(path: string): Promise<string> {
  const daemon = await loadDaemon();
  return daemon.read_vault(path);
}

export async function writeBrowserVault(path: string, body: string): Promise<void> {
  const daemon = await loadDaemon();
  await daemon.write_vault(path, body);
}

export function registerBrowserWorkshopWorker(): void {
  if (typeof navigator === "undefined" || !("serviceWorker" in navigator)) return;
  void navigator.serviceWorker.register("/sw.js").catch(() => {
    // The shell still runs when the worker cannot be installed.
  });
}
