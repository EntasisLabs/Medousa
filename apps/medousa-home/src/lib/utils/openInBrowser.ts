/** Open a URL in the embedded Web surface (desktop) or Web tab (mobile). */

import { layout } from "$lib/stores/layout.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { isTauri } from "$lib/window";

export async function openInBrowser(
  url: string,
  _options?: {
    openedBy?: "agent" | "user";
    sessionId?: string | null;
    workCardId?: string | null;
    title?: string;
    openWorkshop?: boolean;
  },
) {
  const trimmed = url.trim();
  if (!trimmed) return;

  if (layout.isMobile) {
    layout.openWeb();
  } else if (isTauri()) {
    layout.navigateDesktop("web");
  } else {
    return;
  }

  const normalized = trimmed.startsWith("http") ? trimmed : `https://${trimmed}`;
  await humanBrowser.navigate(normalized);

  if (layout.isMobile && _options?.openWorkshop) {
    const { browserWorkshop } = await import("$lib/stores/browserWorkshop.svelte");
    const { browser } = await import("$lib/stores/browser.svelte");
    browserWorkshop.openForBrowser({
      sessionId: _options?.sessionId ?? null,
      tabGroupId: browser.tabGroupId,
      scopeLabel: humanBrowser.scopeLabel,
    });
  }
}

export function isHttpUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

/** Switch to the Web surface without navigating. */
export async function openBrowserWindow() {
  if (layout.isMobile) {
    layout.openWeb();
    return;
  }
  if (!isTauri()) return;
  layout.navigateDesktop("web", { bump: true });
}
