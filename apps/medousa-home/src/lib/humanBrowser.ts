/** Desktop human browser invoke API (no agent bridge). */

import { invoke } from "@tauri-apps/api/core";
import { isTauriMobilePlatform } from "$lib/platform";
import { isPopoutBrowserChrome } from "$lib/stores/humanBrowserSurface";
import type { HumanBrowserSurface } from "$lib/stores/humanBrowserSurface";

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
  canGoBack: boolean;
  canGoForward: boolean;
}

export interface FindInPageResult {
  found: boolean;
}

export async function humanBrowserEmbedSetBounds(
  bounds: HumanBrowserEmbedBounds,
): Promise<void> {
  return invoke("human_browser_embed_set_bounds", { bounds });
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
): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_activate_tab", { tabId, url });
  }
  if (isTauriMobilePlatform()) {
    const trimmed = url.trim();
    if (!trimmed || trimmed === "about:blank") return;
    return humanBrowserNavigate(trimmed);
  }
  return invoke("human_browser_embed_activate_tab", { tabId, url });
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

export async function humanBrowserNavigate(url: string): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_navigate", { url });
  }
  return invoke("human_browser_navigate", { url });
}

export async function humanBrowserReload(): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_reload");
  }
  return invoke("human_browser_reload");
}

export async function humanBrowserGoBack(): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_go_back");
  }
  return invoke("human_browser_go_back");
}

export async function humanBrowserGoForward(): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_go_forward");
  }
  return invoke("human_browser_go_forward");
}

export async function humanBrowserStop(): Promise<void> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_stop");
  }
  return invoke("human_browser_stop");
}

export async function humanBrowserQueryNavState(): Promise<HumanBrowserNavState> {
  if (isPopoutBrowserChrome()) {
    return invoke("human_browser_popout_query_nav_state");
  }
  return invoke("human_browser_query_nav_state");
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

export async function humanBrowserSetMobileShellActive(active: boolean): Promise<void> {
  return invoke("human_browser_set_mobile_shell_active", { active });
}

export interface HumanBrowserSnapshotMarkdown {
  url: string;
  title: string;
  markdown: string;
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

export interface HumanBrowserNavigatedPayload {
  url: string;
  title?: string | null;
  favicon?: string | null;
  tabId?: string | null;
  surface?: HumanBrowserSurface;
}

export interface HumanBrowserLoadingPayload {
  loading: boolean;
  surface?: HumanBrowserSurface;
}

export interface HumanBrowserNavStatePayload {
  canGoBack: boolean;
  canGoForward: boolean;
  surface?: HumanBrowserSurface;
}
