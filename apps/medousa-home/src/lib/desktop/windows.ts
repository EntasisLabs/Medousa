import { invoke } from "@tauri-apps/api/core";
import { isTauriDesktop } from "$lib/platform";

export type DesktopTarget = {
  kind: "chat" | "create" | "code" | "terminal";
  workshopId: string;
  resourceId?: string | null;
  sshTargetId?: string | null;
  path?: string | null;
  title?: string | null;
};

export async function openDesktopWindow(target: DesktopTarget): Promise<string> {
  if (!isTauriDesktop()) throw new Error("Desktop windows are available in the Medousa desktop app");
  return invoke<string>("desktop_open_window", { target });
}

export function toggleDesktopSpotlight(): Promise<void> {
  return invoke("desktop_toggle_spotlight");
}

/** Detached views may navigate internally, but never persist the main tab layout. */
export function isDesktopWindow(): boolean {
  return typeof window !== "undefined" && window.location.pathname.startsWith("/desktop/");
}
