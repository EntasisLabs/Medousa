/** Open a URL on the Web surface (shared browser workspace). */

import { browser } from "$lib/stores/browser.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { chat } from "$lib/stores/chat.svelte";

function navigateToWebSurface() {
  if (layout.isMobile) {
    layout.openYou("web");
    return;
  }
  layout.navigateDesktop("web", { bump: true });
}

export async function openInBrowser(
  url: string,
  options?: {
    openedBy?: "agent" | "user";
    sessionId?: string | null;
    workCardId?: string | null;
    title?: string;
    openWorkshop?: boolean;
  },
) {
  const trimmed = url.trim();
  if (!trimmed) return;

  navigateToWebSurface();

  const sessionId = options?.sessionId?.trim() || chat.sessionId?.trim() || null;
  if (sessionId) {
    browser.linkSession(sessionId);
    if (sessionId !== chat.sessionId) {
      await chat.switchSession(sessionId);
    }
  }

  if (options?.workCardId) {
    await browser.linkWorkCard(options.workCardId);
  }

  await browser.navigate(trimmed, options?.openedBy ?? "user", options?.title);

  if (options?.openWorkshop) {
    const { browserWorkshop } = await import("$lib/stores/browserWorkshop.svelte");
    browserWorkshop.openForBrowser({
      sessionId,
      tabGroupId: browser.tabGroupId,
      scopeLabel: browser.scopeLabel,
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
