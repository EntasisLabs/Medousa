/** Open floating browser workshop scoped to tab group + chat session. */

import { browser } from "$lib/stores/browser.svelte";
import { browserWorkshop } from "$lib/stores/browserWorkshop.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { layout } from "$lib/stores/layout.svelte";

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
    layout.openYou("web");
    browser.linkSession(sessionId);
    await browser.ensureTabGroup(sessionId);
    if (input?.navigateUrl?.trim()) {
      await browser.navigate(input.navigateUrl.trim(), "agent");
    }
    browserWorkshop.openForBrowser({
      sessionId,
      tabGroupId: browser.tabGroupId,
      scopeLabel: browser.scopeLabel,
    });
    if (input?.openMinimized) browserWorkshop.minimized = true;
    return;
  }

  // Desktop human-first browser: open main chat until agent workshop reattaches.
  layout.navigateDesktop("chat", { bump: true });
  void chat.ensureSessionHydrated();
}
