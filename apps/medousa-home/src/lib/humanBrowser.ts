/** Desktop human browser invoke API (no agent bridge). */

import { invoke } from "@tauri-apps/api/core";

export interface HumanBrowserEmbedLayout {
  activityWidth: number;
  activityCollapsed: boolean;
  workRailVisible: boolean;
}

export interface HumanBrowserEmbedBounds {
  x: number;
  y: number;
  width: number;
  height: number;
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
}> {
  return invoke("human_browser_embed_read_bounds");
}

export async function humanBrowserNavigate(url: string): Promise<void> {
  return invoke("human_browser_navigate", { url });
}

export async function humanBrowserReload(): Promise<void> {
  return invoke("human_browser_reload");
}

export async function humanBrowserGoBack(): Promise<void> {
  return invoke("human_browser_go_back");
}

export async function humanBrowserGoForward(): Promise<void> {
  return invoke("human_browser_go_forward");
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

export interface HumanBrowserNavigatedPayload {
  url: string;
  title?: string | null;
}
