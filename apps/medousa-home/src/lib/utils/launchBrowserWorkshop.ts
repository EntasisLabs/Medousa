/** Open floating browser workshop scoped to tab group + chat session. */

import { browser } from "$lib/stores/browser.svelte";
import { browserWorkshop } from "$lib/stores/browserWorkshop.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { openInBrowser } from "$lib/utils/openInBrowser";

export async function launchBrowserWorkshop(input?: {
  sessionId?: string | null;
  navigateUrl?: string | null;
  openMinimized?: boolean;
}) {
  const sessionId = input?.sessionId?.trim() || chat.sessionId?.trim() || null;
  if (sessionId && sessionId !== chat.sessionId) {
    await chat.switchSession(sessionId);
  } else {
    void chat.ensureSessionHydrated();
  }

  if (layout.isMobile) {
    layout.openWeb();
    browser.linkSession(sessionId);
    await browser.ensureTabGroup(sessionId);
    if (input?.navigateUrl?.trim()) {
      await openInBrowser(input.navigateUrl.trim(), {
        openedBy: "agent",
        sessionId,
      });
    }
    browserWorkshop.openForBrowser({
      sessionId,
      tabGroupId: browser.tabGroupId,
      scopeLabel: humanBrowser.scopeLabel,
    });
    if (input?.openMinimized) browserWorkshop.minimized = true;
    return;
  }

  layout.navigateDesktop("web");
  browser.linkSession(sessionId);
  await browser.ensureTabGroup(sessionId);
  if (input?.navigateUrl?.trim()) {
    await openInBrowser(input.navigateUrl.trim(), {
      openedBy: "agent",
      sessionId,
    });
  }
  browserWorkshop.openForBrowser({
    sessionId,
    tabGroupId: browser.tabGroupId,
    scopeLabel: humanBrowser.scopeLabel,
  });
  if (input?.openMinimized) browserWorkshop.minimized = true;
}
