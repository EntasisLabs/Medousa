import type { OperationId } from "$lib/daemon/generatedOps";
import { OPERATIONS } from "$lib/daemon/generatedOps";
import { isBrowserWorkshop } from "$lib/platform";
import {
  PERSONAL_WORKSHOP_ID,
  type WorkshopRegistry,
} from "$lib/types/workshopRegistry";
import { loadDaemon } from "$lib/wasm/browserDaemon";

const REGISTRY_KEY = "medousa.browser.workshop-registry";
const TURN_STREAM_ACCEPT = "text/event-stream; medousa-version=3";

type StreamKind = "interactive" | "workspace" | "environment";

const streamListeners: Record<StreamKind, Set<(event: unknown) => void>> = {
  interactive: new Set(),
  workspace: new Set(),
  environment: new Set(),
};
const streamErrors: Record<StreamKind, Set<(message: string) => void>> = {
  interactive: new Set(),
  workspace: new Set(),
  environment: new Set(),
};

export function loadBrowserWorkshopRegistry(): WorkshopRegistry | null {
  if (typeof localStorage === "undefined") return null;
  const raw = localStorage.getItem(REGISTRY_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as WorkshopRegistry;
  } catch {
    return null;
  }
}

export function saveBrowserWorkshopRegistry(registry: WorkshopRegistry): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(REGISTRY_KEY, JSON.stringify(registry));
}

export function browserPortalActive(): boolean {
  if (!isBrowserWorkshop()) return false;
  const id = loadBrowserWorkshopRegistry()?.activeWorkshopId;
  return Boolean(id && id !== PERSONAL_WORKSHOP_ID);
}

export function expandOperationPath(
  path: string,
  pathParams: Record<string, string> = {},
  query?: Record<string, string>,
): string {
  const expanded = path.replace(/\{(\*?)([^}]+)\}/g, (_match, star: string, name: string) => {
    const value = pathParams[name];
    if (value == null || value === "") {
      throw new Error(`missing path parameter ${name}`);
    }
    if (star) {
      return value
        .split("/")
        .map((part) => encodeURIComponent(part))
        .join("/");
    }
    return encodeURIComponent(value);
  });
  if (!query) return expanded;
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined && value !== "") params.set(key, value);
  }
  const text = params.toString();
  return text ? `${expanded}?${text}` : expanded;
}

export function streamPathFromUrl(streamUrl: string): string {
  try {
    const url = new URL(streamUrl, "http://medousa.workshop");
    return `${url.pathname}${url.search}`;
  } catch {
    return streamUrl.startsWith("/") ? streamUrl : `/${streamUrl}`;
  }
}

export interface BrowserPairResult {
  pairingId: string;
  phoneId: string;
  workshopDeviceId: string;
  workshopId: string;
  workshopPeerName: string;
  daemonUrl: string;
}

export async function pairBrowserFromInvite(
  qrUrl: string,
  displayName = "Medousa",
): Promise<BrowserPairResult> {
  const daemon = await loadDaemon();
  return JSON.parse(await daemon.pair_from_invite(qrUrl, displayName)) as BrowserPairResult;
}

export async function activateBrowserPortal(workshopId: string): Promise<void> {
  const daemon = await loadDaemon();
  daemon.set_active_portal(workshopId);
}

export async function forgetBrowserPortal(workshopId: string): Promise<void> {
  const daemon = await loadDaemon();
  daemon.forget_portal(workshopId);
}

export async function portalRequest<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const daemon = await loadDaemon();
  const encoded = body === undefined ? "" : JSON.stringify(body);
  const raw = JSON.parse(await daemon.portal_request(method, path, encoded)) as {
    status: number;
    body: string;
  };
  if (raw.status < 200 || raw.status >= 300) {
    throw new Error(`workshop returned HTTP ${raw.status}: ${raw.body}`);
  }
  if (!raw.body.trim()) return undefined as T;
  return JSON.parse(raw.body) as T;
}

export async function portalOperation<T>(
  id: OperationId,
  pathParams: Record<string, string> = {},
  body?: unknown,
  query?: Record<string, string>,
): Promise<T> {
  const operation = OPERATIONS[id];
  if (operation.streaming) {
    throw new Error(`use a portal stream for ${id}`);
  }
  return portalRequest<T>(
    operation.method,
    expandOperationPath(operation.path, pathParams, query),
    body,
  );
}

export function subscribePortalStream<T>(
  kind: StreamKind,
  handler: (event: T) => void,
): () => void {
  const listeners = streamListeners[kind] as Set<(event: T) => void>;
  listeners.add(handler);
  return () => listeners.delete(handler);
}

export function subscribePortalStreamError(
  kind: StreamKind,
  handler: (message: string) => void,
): () => void {
  streamErrors[kind].add(handler);
  return () => streamErrors[kind].delete(handler);
}

export async function openPortalStream(
  kind: string,
  path: string,
  accept = "",
  handlers?: {
    onEvent?: (event: unknown) => void;
    onError?: (message: string) => void;
  },
): Promise<void> {
  const daemon = await loadDaemon();
  const bus = streamKind(kind);
  daemon.portal_open_stream(
    kind,
    path,
    accept,
    (data) => {
      let event: unknown;
      try {
        event = JSON.parse(data);
      } catch {
        return;
      }
      handlers?.onEvent?.(event);
      if (!bus) return;
      for (const listener of streamListeners[bus]) listener(event);
    },
    (message) => {
      handlers?.onError?.(message);
      if (!bus) return;
      for (const listener of streamErrors[bus]) listener(message);
    },
  );
}

export function stopPortalStreams(prefix: string): void {
  void loadDaemon()
    .then((daemon) => daemon.portal_stop_streams(prefix))
    .catch(() => undefined);
}

export function openPortalInteractiveStream(streamUrl: string): Promise<void> {
  const path = streamPathFromUrl(streamUrl);
  const turnId = path.split("/").filter(Boolean).at(-2) ?? "turn";
  return openPortalStream(`interactive:${turnId}`, path, TURN_STREAM_ACCEPT);
}

export function stopPortalInteractiveStreams(): void {
  stopPortalStreams("interactive:");
}

function streamKind(kind: string): StreamKind | null {
  if (kind.startsWith("interactive")) return "interactive";
  if (kind === "workspace" || kind === "environment") return kind;
  return null;
}
