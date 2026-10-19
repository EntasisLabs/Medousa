/** Desktop human browser invoke API (no agent bridge). */

import { invoke } from "@tauri-apps/api/core";
import { isTauriMobilePlatform } from "$lib/platform";
import { isPopoutBrowserChrome, type HumanBrowserSurface } from "$lib/utils/humanBrowserWindow";

export type { HumanBrowserSurface };

export interface HumanBrowserEmbedLayout {
  activityWidth: number;
  activityCollapsed: boolean;
  workRailVisible: boolean;
  /** Measured chrome bottom in shell viewport (`getBoundingClientRect().bottom`). */
  contentTop?: number;
}

export interface HumanBrowserEmbedBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HumanBrowserNavState {
  tabId?: string | null;
  url?: string | null;
  loading?: boolean | null;
  canGoBack: boolean;
  canGoForward: boolean;
}

export interface FindInPageResult {
  found: boolean;
}

export async function humanBrowserEmbedSetBounds(
  bounds: HumanBrowserEmbedBounds,
  zoom?: number,
): Promise<void> {
  return invoke("human_browser_embed_set_bounds", { bounds, zoom });
}

export async function humanBrowserEmbedApplyLayout(
  params: HumanBrowserEmbedLayout,
): Promise<void> {
  return invoke("human_browser_embed_apply_layout", { params });
}

export interface HumanBrowserEmbedMobileLayout {
  bottomChromeHeight: number;
  contentBounds?: HumanBrowserEmbedBounds | null;
}

export async function humanBrowserEmbedApplyMobileLayout(
  params: HumanBrowserEmbedMobileLayout,
): Promise<boolean> {
  return invoke("human_browser_embed_apply_mobile_layout", { params });
}

export async function humanBrowserEmbedReadBounds(): Promise<HumanBrowserEmbedBounds & {
  windowWidth: number;
  windowHeight: number;
  shellOriginX: number;
  shellOriginY: number;
}> {
  return invoke("human_browser_embed_read_bounds");
}

export async function humanBrowserEmbedCoordProbe(
  dom?: HumanBrowserEmbedBounds | null,
): Promise<Record<string, unknown>> {
  return invoke("human_browser_embed_coord_probe", { dom: dom ?? null });
}

export async function humanBrowserActivateTab(
  tabId: string,
  url: string,
  navigate = false,
): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_activate_tab", { tabId, url, navigate });
  }
  if (isTauriMobilePlatform()) {
    const trimmed = url.trim();
    if (!trimmed || trimmed === "about:blank") return;
    return humanBrowserNavigate(trimmed);
  }
  return invoke("human_browser_embed_activate_tab", { tabId, url, navigate });
}

export async function humanBrowserCloseTab(tabId: string): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_close_tab", { tabId });
  }
  if (isTauriMobilePlatform()) {
    return;
  }
  return invoke("human_browser_embed_close_tab", { tabId });
}

export async function humanBrowserNavigate(url: string, tabId?: string): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_navigate", { url, tabId });
  }
  return invoke("human_browser_navigate", { url, tabId });
}

export async function humanBrowserReload(tabId?: string): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_reload", { tabId });
  }
  return invoke("human_browser_reload", { tabId });
}

export async function humanBrowserGoBack(tabId?: string): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_go_back", { tabId });
  }
  return invoke("human_browser_go_back", { tabId });
}

export async function humanBrowserGoForward(tabId?: string): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_go_forward", { tabId });
  }
  return invoke("human_browser_go_forward", { tabId });
}

export async function humanBrowserStop(tabId?: string): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_stop", { tabId });
  }
  return invoke("human_browser_stop", { tabId });
}

export async function humanBrowserQueryNavState(tabId?: string): Promise<HumanBrowserNavState> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_query_nav_state", { tabId });
  }
  return invoke("human_browser_query_nav_state", { tabId });
}

export async function humanBrowserFindInPage(
  query: string,
  forward = true,
): Promise<FindInPageResult> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_find_in_page", { query, forward });
  }
  return invoke("human_browser_find_in_page", { query, forward });
}

export async function humanBrowserEmbedShow(): Promise<void> {
  return invoke("human_browser_embed_show");
}

export async function humanBrowserEmbedHide(): Promise<void> {
  return invoke("human_browser_embed_hide");
}

export async function humanBrowserPopoutSetChromeOverlay(active: boolean): Promise<void> {
  return invoke("human_browser_popout_set_chrome_overlay", { active });
}

export async function humanBrowserSetMobileShellActive(active: boolean): Promise<void> {
  return invoke("human_browser_set_mobile_shell_active", { active });
}

export interface HumanBrowserSnapshotMarkdown {
  url: string;
  title: string;
  markdown: string;
  truncated?: boolean;
}

export interface HumanBrowserSearchSnapshot {
  query: string;
  provider: string;
  results: Array<{ title: string; url: string; snippet: string }>;
  cached: boolean;
  challenge?: string | null;
}

export async function humanBrowserSnapshotSearch(
  query: string,
  maxResults = 8,
): Promise<HumanBrowserSearchSnapshot> {
  return invoke("human_browser_snapshot_search", { query, maxResults });
}

export async function humanBrowserSnapshotMarkdown(
  maxChars = 4000,
): Promise<HumanBrowserSnapshotMarkdown> {
  return invoke("human_browser_snapshot_markdown", { maxChars });
}

export interface HumanBrowserActRequest {
  action: string;
  selector?: string | null;
  text?: string | null;
  key?: string | null;
  value?: string | null;
  delta_y?: number | null;
  ms?: number | null;
}

export interface HumanBrowserActReport {
  ok: boolean;
  url: string;
  error?: string | null;
}

/** Mobile (iOS/Android): run a click/type/scroll/select act in the overlay webview. */
export async function humanBrowserAct(
  request: HumanBrowserActRequest,
): Promise<HumanBrowserActReport> {
  return invoke("human_browser_act", { request });
}

export interface HumanBrowserNavigatedPayload {
  url: string;
  title?: string | null;
  favicon?: string | null;
  tabId?: string | null;
  surface?: HumanBrowserSurface;
  /** Same-document / History API change — update URL without shell history push. */
  inPage?: boolean;
}

export interface HumanBrowserLoadingPayload {
  tabId?: string | null;
  loading: boolean;
  surface?: HumanBrowserSurface;
}

export interface HumanBrowserNavStatePayload {
  tabId?: string | null;
  canGoBack: boolean;
  canGoForward: boolean;
  surface?: HumanBrowserSurface;
}
