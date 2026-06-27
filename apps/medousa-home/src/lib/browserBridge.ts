/** Browser tab group API — Tauri invoke in desktop app (no CORS); HTTP fallback for web-only dev. */

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/platform";

export type TabOpenedBy = "agent" | "user";
export type BrowserControl = "agent" | "user" | "awaiting_operator";

export interface BrowserTab {
  id: string;
  url: string;
  title: string;
  favicon?: string | null;
  opened_by: TabOpenedBy;
  active: boolean;
}

export interface TabGroup {
  id: string;
  chat_session_id?: string | null;
  work_card_id?: string | null;
  tabs: BrowserTab[];
  control: BrowserControl;
}

export function isLocalTabGroupId(tabGroupId: string): boolean {
  return tabGroupId.startsWith("tg-local-");
}

function useTauriBridge(): boolean {
  return isTauri() && !isTauriMobilePlatform();
}

export async function browserHostBaseUrl(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    const status = await invoke<{ baseUrl: string; healthy: boolean }>("browser_host_status");
    return status.healthy ? status.baseUrl : null;
  } catch {
    return null;
  }
}

async function bridgeFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const base = await browserHostBaseUrl();
  if (!base) {
    throw new Error("BrowserHost is not available");
  }
  const response = await fetch(`${base.replace(/\/$/, "")}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(text || `BrowserHost HTTP ${response.status}`);
  }
  return response.json() as Promise<T>;
}

export async function bridgeCreateTabGroup(input?: {
  chatSessionId?: string;
  workCardId?: string;
}): Promise<TabGroup> {
  if (useTauriBridge()) {
    return invoke<TabGroup>("browser_bridge_create_tab_group", {
      chatSessionId: input?.chatSessionId ?? null,
      workCardId: input?.workCardId ?? null,
    });
  }
  return bridgeFetch<TabGroup>("/v1/tab-groups", {
    method: "POST",
    body: JSON.stringify({
      chat_session_id: input?.chatSessionId ?? null,
      work_card_id: input?.workCardId ?? null,
    }),
  });
}

export async function bridgeGetTabGroup(tabGroupId: string): Promise<TabGroup | null> {
  if (isLocalTabGroupId(tabGroupId)) return null;
  if (useTauriBridge()) {
    return invoke<TabGroup | null>("browser_bridge_get_tab_group", { tabGroupId });
  }
  const base = await browserHostBaseUrl();
  if (!base) return null;
  const response = await fetch(
    `${base.replace(/\/$/, "")}/v1/tab-groups/${encodeURIComponent(tabGroupId)}`,
  );
  if (!response.ok) return null;
  const body = (await response.json()) as { ok?: boolean; tab_group?: TabGroup };
  return body.ok ? body.tab_group ?? null : null;
}

export async function bridgeOpenTab(
  tabGroupId: string,
  url: string,
  openedBy: TabOpenedBy = "user",
  title?: string,
): Promise<TabGroup | null> {
  if (isLocalTabGroupId(tabGroupId)) return null;
  if (useTauriBridge()) {
    try {
      return await invoke<TabGroup>("browser_bridge_open_tab", {
        tabGroupId,
        url,
        openedBy,
        title: title ?? null,
      });
    } catch {
      return null;
    }
  }
  const body = await bridgeFetch<{ ok: boolean; tab_group?: TabGroup }>(
    `/v1/tab-groups/${encodeURIComponent(tabGroupId)}/tabs`,
    {
      method: "POST",
      body: JSON.stringify({ url, title: title ?? null, opened_by: openedBy }),
    },
  );
  return body.ok ? body.tab_group ?? null : null;
}

export async function bridgeNavigate(
  tabGroupId: string,
  url: string,
  openedBy: TabOpenedBy = "user",
  title?: string,
): Promise<TabGroup | null> {
  if (isLocalTabGroupId(tabGroupId)) return null;
  if (useTauriBridge()) {
    try {
      return await invoke<TabGroup>("browser_bridge_navigate_tab", {
        tabGroupId,
        url,
        openedBy,
        title: title ?? null,
      });
    } catch {
      return null;
    }
  }
  const body = await bridgeFetch<{ ok: boolean; tab_group?: TabGroup }>(
    `/v1/tab-groups/${encodeURIComponent(tabGroupId)}/navigate`,
    {
      method: "POST",
      body: JSON.stringify({ url, title: title ?? null, opened_by: openedBy }),
    },
  );
  return body.ok ? body.tab_group ?? null : null;
}

export async function bridgeActivateTab(
  tabGroupId: string,
  tabId: string,
): Promise<TabGroup | null> {
  if (isLocalTabGroupId(tabGroupId)) return null;
  if (useTauriBridge()) {
    try {
      return await invoke<TabGroup>("browser_bridge_activate_tab", { tabGroupId, tabId });
    } catch {
      return null;
    }
  }
  const body = await bridgeFetch<{ ok: boolean; tab_group?: TabGroup }>(
    `/v1/tab-groups/${encodeURIComponent(tabGroupId)}/tabs/${encodeURIComponent(tabId)}/activate`,
    { method: "POST", body: "{}" },
  );
  return body.ok ? body.tab_group ?? null : null;
}

export async function bridgeCloseTab(
  tabGroupId: string,
  tabId: string,
): Promise<TabGroup | null> {
  if (isLocalTabGroupId(tabGroupId)) return null;
  if (useTauriBridge()) {
    try {
      return await invoke<TabGroup>("browser_bridge_close_tab", { tabGroupId, tabId });
    } catch {
      return null;
    }
  }
  const body = await bridgeFetch<{ ok: boolean; tab_group?: TabGroup }>(
    `/v1/tab-groups/${encodeURIComponent(tabGroupId)}/tabs/${encodeURIComponent(tabId)}`,
    { method: "DELETE" },
  );
  return body.ok ? body.tab_group ?? null : null;
}

export async function bridgeSetControl(
  tabGroupId: string,
  control: BrowserControl,
): Promise<TabGroup | null> {
  if (isLocalTabGroupId(tabGroupId)) return null;
  if (useTauriBridge()) {
    try {
      return await invoke<TabGroup>("browser_bridge_set_control", { tabGroupId, control });
    } catch {
      return null;
    }
  }
  const body = await bridgeFetch<{ ok: boolean; tab_group?: TabGroup }>(
    `/v1/tab-groups/${encodeURIComponent(tabGroupId)}/control`,
    {
      method: "POST",
      body: JSON.stringify({ control }),
    },
  );
  return body.ok ? body.tab_group ?? null : null;
}

export async function bridgeLinkWorkCard(
  tabGroupId: string,
  workCardId: string | null,
): Promise<TabGroup | null> {
  if (isLocalTabGroupId(tabGroupId)) return null;
  if (useTauriBridge()) {
    try {
      return await invoke<TabGroup>("browser_bridge_link_work_card", {
        tabGroupId,
        workCardId,
      });
    } catch {
      return null;
    }
  }
  const body = await bridgeFetch<{ ok: boolean; tab_group?: TabGroup }>(
    `/v1/tab-groups/${encodeURIComponent(tabGroupId)}/link-work`,
    {
      method: "POST",
      body: JSON.stringify({ work_card_id: workCardId }),
    },
  );
  return body.ok ? body.tab_group ?? null : null;
}

export async function bridgeSnapshot(
  tabGroupId: string,
  maxChars = 4000,
): Promise<{ url: string; title: string; markdown: string } | null> {
  if (isLocalTabGroupId(tabGroupId)) return null;
  if (useTauriBridge()) {
    try {
      return await invoke<{ url: string; title: string; markdown: string }>(
        "browser_bridge_snapshot",
        { tabGroupId, maxChars },
      );
    } catch {
      return null;
    }
  }
  const base = await browserHostBaseUrl();
  if (!base) return null;
  const response = await fetch(
    `${base.replace(/\/$/, "")}/v1/tab-groups/${encodeURIComponent(tabGroupId)}/snapshot`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ max_chars: maxChars }),
    },
  );
  if (!response.ok) return null;
  return response.json() as Promise<{ url: string; title: string; markdown: string }>;
}
