/** Open a URL in the embedded Web surface (desktop) or Web tab (mobile). */

import { browser } from "$lib/stores/browser.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { markAgentNavigation } from "$lib/utils/agentBrowserCoord";
import { isTauri } from "$lib/window";

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

  if (layout.isMobile) {
    layout.openWeb();
  } else if (isTauri()) {
    layout.navigateDesktop("web");
  } else {
    return;
  }

  const openedBy = options?.openedBy ?? "user";
  const sessionId =
    options?.sessionId?.trim() || chat.sessionId?.trim() || null;

  if (sessionId) {
    browser.linkSession(sessionId);
  }
  if (options?.workCardId?.trim()) {
    await browser.linkWorkCard(options.workCardId.trim());
  }
  await browser.ensureTabGroup(sessionId);

  const normalized = trimmed.startsWith("http") ? trimmed : `https://${trimmed}`;

  if (openedBy === "agent") {
    markAgentNavigation();
    await browser.handleAgentNavigation(normalized, options?.title);
  }

  await humanBrowser.navigate(normalized);

  if (openedBy === "user") {
    await browser.syncFromNative(normalized);
  } else if (browser.control !== "awaiting_operator") {
    await browser.setControl("agent");
  }

  if (options?.openWorkshop) {
    if (layout.isMobile) {
      const { browserWorkshop } = await import("$lib/stores/browserWorkshop.svelte");
      browserWorkshop.openForBrowser({
        sessionId,
        tabGroupId: browser.tabGroupId,
        scopeLabel: humanBrowser.scopeLabel,
      });
    } else {
      const { launchBrowserWorkshop } = await import("$lib/utils/launchBrowserWorkshop");
      await launchBrowserWorkshop({
        sessionId,
        navigateUrl: null,
      });
    }
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
