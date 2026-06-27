/** Open a URL in the embedded Web surface (desktop) or You → Web (mobile). */

import { browser } from "$lib/stores/browser.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { humanBrowserNavigate } from "$lib/humanBrowser";
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
    layout.openYou("web");
    await browser.navigate(trimmed, "user", _options?.title);
    if (_options?.openWorkshop) {
      const { browserWorkshop } = await import("$lib/stores/browserWorkshop.svelte");
      browserWorkshop.openForBrowser({
        sessionId: _options?.sessionId ?? null,
        tabGroupId: browser.tabGroupId,
        scopeLabel: browser.scopeLabel,
      });
    }
    return;
  }

  if (!isTauri()) return;

  layout.navigateDesktop("web");
  const normalized = trimmed.startsWith("http") ? trimmed : `https://${trimmed}`;
  await humanBrowserNavigate(normalized);
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
    layout.openYou("web");
    return;
  }
  if (!isTauri()) return;
  layout.navigateDesktop("web", { bump: true });
}
