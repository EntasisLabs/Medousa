import { invoke } from "@tauri-apps/api/core";

export async function showChatPopout(): Promise<void> {
  return invoke("window_show_chat_popout");
}

export async function hideChatPopout(): Promise<void> {
  return invoke("window_hide_chat_popout");
}

export async function showVaultSticky(): Promise<void> {
  return invoke("window_show_vault_sticky");
}

export async function hideVaultSticky(): Promise<void> {
  return invoke("window_hide_vault_sticky");
}

export async function setVaultStickyAlwaysOnTop(alwaysOnTop: boolean): Promise<void> {
  return invoke("window_set_vault_sticky_always_on_top", { alwaysOnTop });
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

export async function setVaultStickyWindowTitle(title: string): Promise<void> {
  if (!isTauri()) return;
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const stickyWin = await WebviewWindow.getByLabel("vault-sticky");
  if (!stickyWin) return;
  await stickyWin.setTitle(title);
}
