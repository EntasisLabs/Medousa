/** Everyday browser chrome actions (copy URL, open externally). */

import { isTauri } from "$lib/platform";

export async function copyBrowserUrl(url: string): Promise<boolean> {
  const trimmed = url.trim();
  if (!trimmed || trimmed === "about:blank") return false;
  try {
    await navigator.clipboard.writeText(trimmed);
    return true;
  } catch {
    return false;
  }
}

export async function openUrlInDefaultBrowser(url: string): Promise<boolean> {
  const trimmed = url.trim();
  if (!trimmed || trimmed === "about:blank") return false;
  try {
    if (isTauri()) {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(trimmed);
      return true;
    }
    window.open(trimmed, "_blank", "noopener,noreferrer");
    return true;
  } catch {
    return false;
  }
}
