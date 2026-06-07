import { invoke } from "@tauri-apps/api/core";

export async function showChatPopout(): Promise<void> {
  return invoke("window_show_chat_popout");
}

export async function hideChatPopout(): Promise<void> {
  return invoke("window_hide_chat_popout");
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function updateTrayBlockedCount(blockedCount: number): Promise<void> {
  if (!isTauri()) return;
  return invoke("tray_update_blocked_count", { blockedCount });
}
