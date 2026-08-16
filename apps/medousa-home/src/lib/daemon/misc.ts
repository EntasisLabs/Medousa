import { invoke } from "@tauri-apps/api/core";
import type {
  LocusNodeDetailResponse,
  LocusNodesListResponse,
  LocusTagsListResponse,
} from "$lib/types/locus";
import { getDaemonUrl } from "./client";

export async function resumeBrowserHostSession(
  sessionId: string,
): Promise<Record<string, unknown>> {
  const daemonUrl = (await getDaemonUrl()).replace(/\/$/, "");
  return invoke<Record<string, unknown>>("browser_host_resume_session", {
    sessionId,
    daemonUrl,
  });
}

/** Resume browser session after operator verification (desktop + mobile). */
export async function resumeBrowserSession(
  sessionId: string,
): Promise<Record<string, unknown>> {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    const { isTauriMobilePlatform } = await import("$lib/platform");
    if (!isTauriMobilePlatform()) {
      const { resumeBrowserChallenge } = await import("$lib/utils/resumeBrowserChallenge");
      await resumeBrowserChallenge(sessionId);
      return { ok: true, session_id: sessionId };
    }
  }
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  const response = await fetch(
    `${base}/v1/browser/sessions/${encodeURIComponent(sessionId)}/resume`,
    { method: "POST", headers: { "Content-Type": "application/json" } },
  );
  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(text || `HTTP ${response.status}`);
  }
  return response.json() as Promise<Record<string, unknown>>;
}

export async function registerBrowserClient(
  daemonUrl: string,
  channelSurface: string,
): Promise<void> {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    await invoke("browser_host_register_client", { daemonUrl, channelSurface });
    return;
  }
  const base = daemonUrl.replace(/\/$/, "");
  await fetch(`${base}/v1/clients/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      client_id: `home-${channelSurface}`,
      channel_surface: channelSurface,
      supports_browser_host: true,
    }),
  });
}

export async function completeBrowserSession(
  sessionId: string,
  payload: {
    searchResponse?: unknown;
    error?: string | null;
  },
): Promise<Record<string, unknown>> {
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  const response = await fetch(
    `${base}/v1/browser/sessions/${encodeURIComponent(sessionId)}/complete`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        search_response: payload.searchResponse ?? null,
        error: payload.error ?? null,
      }),
    },
  );
  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(text || `HTTP ${response.status}`);
  }
  return response.json() as Promise<Record<string, unknown>>;
}

export interface BrowserActRequestPayload {
  action: string;
  selector?: string | null;
  text?: string | null;
  key?: string | null;
  value?: string | null;
  delta_y?: number | null;
  ms?: number | null;
}

export interface BrowserSessionRecord {
  session_id: string;
  query: string;
  max_results: number;
  status: string;
  act_request?: BrowserActRequestPayload | null;
}

export async function completeBrowserActSession(
  sessionId: string,
  outcome: { ok: boolean; url?: string; error?: string | null },
): Promise<Record<string, unknown>> {
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  const response = await fetch(
    `${base}/v1/browser/sessions/${encodeURIComponent(sessionId)}/complete-act`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ok: outcome.ok,
        url: outcome.url ?? "",
        error: outcome.error ?? null,
      }),
    },
  );
  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(text || `HTTP ${response.status}`);
  }
  return response.json() as Promise<Record<string, unknown>>;
}

export async function fetchBrowserSession(
  sessionId: string,
): Promise<BrowserSessionRecord> {
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  const response = await fetch(
    `${base}/v1/browser/sessions/${encodeURIComponent(sessionId)}`,
  );
  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(text || `HTTP ${response.status}`);
  }
  const body = (await response.json()) as {
    ok?: boolean;
    session?: BrowserSessionRecord;
    error?: string;
  };
  if (!body.ok || !body.session) {
    throw new Error(body.error || "browser session not found");
  }
  return body.session;
}

export async function listLocusNodes(options?: {
  sessionId?: string;
  limit?: number;
  q?: string;
  tags?: string | string[];
  tagPrefix?: string;
}): Promise<LocusNodesListResponse> {
  const tags =
    options?.tags == null
      ? undefined
      : Array.isArray(options.tags)
        ? options.tags.join(",")
        : options.tags;
  return invoke<LocusNodesListResponse>("locus_list_nodes", {
    sessionId: options?.sessionId,
    limit: options?.limit,
    q: options?.q,
    tags,
    tagPrefix: options?.tagPrefix,
  });
}

export async function listLocusTags(options?: {
  sessionId?: string;
  prefix?: string;
  limit?: number;
}): Promise<LocusTagsListResponse> {
  return invoke<LocusTagsListResponse>("locus_list_tags", {
    sessionId: options?.sessionId,
    prefix: options?.prefix,
    limit: options?.limit,
  });
}

export async function getLocusNode(syncKey: string): Promise<LocusNodeDetailResponse> {
  return invoke<LocusNodeDetailResponse>("locus_get_node", { syncKey });
}
