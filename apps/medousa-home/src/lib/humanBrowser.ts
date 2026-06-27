/** Desktop human browser invoke API (no agent bridge). */

import { invoke } from "@tauri-apps/api/core";

export interface HumanBrowserEmbedLayout {
  activityWidth: number;
  activityCollapsed: boolean;
  workRailVisible: boolean;
}

export async function humanBrowserEmbedApplyLayout(
  params: HumanBrowserEmbedLayout,
): Promise<void> {
  return invoke("human_browser_embed_apply_layout", { params });
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

export interface HumanBrowserNavigatedPayload {
  url: string;
  title?: string | null;
}
