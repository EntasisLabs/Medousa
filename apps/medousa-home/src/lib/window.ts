import { invoke } from "@tauri-apps/api/core";

export async function showChatPopout(): Promise<void> {
  return invoke("window_show_chat_popout");
}

export async function hideChatPopout(): Promise<void> {
  return invoke("window_hide_chat_popout");
}

export async function showBrowser(): Promise<void> {
  return invoke("window_show_browser");
}

export async function hideBrowser(): Promise<void> {
  return invoke("window_hide_browser");
}

export async function focusBrowser(): Promise<void> {
  return invoke("window_focus_browser");
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function updateTrayBlockedCount(blockedCount: number): Promise<void> {
  if (!isTauri()) return;
  return invoke("tray_update_blocked_count", { blockedCount });
}

export async function setBrowserWindowTitle(title: string): Promise<void> {
  if (!isTauri()) return;
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const browserWin = await WebviewWindow.getByLabel("browser");
  if (!browserWin) return;
  await browserWin.setTitle(title);
}
