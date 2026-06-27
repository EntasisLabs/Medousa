/** Resume agent browser session after operator solves CAPTCHA in the human webview. */

import { completeBrowserSession, fetchBrowserSession } from "$lib/daemon";
import {
  humanBrowserSnapshotMarkdown,
  humanBrowserSnapshotSearch,
} from "$lib/humanBrowser";
import { browser } from "$lib/stores/browser.svelte";
import { isTauri } from "$lib/window";

function isDdgUrl(url: string): boolean {
  try {
    const host = new URL(url).hostname.toLowerCase();
    return host.includes("duckduckgo.com");
  } catch {
    return url.includes("duckduckgo.com");
  }
}

export async function resumeBrowserChallenge(sessionId: string): Promise<void> {
  const session = await fetchBrowserSession(sessionId);
  const query = session.query?.trim();
  if (!query) {
    throw new Error("browser session missing query");
  }
  const maxResults = session.max_results ?? 8;

  if (isTauri()) {
    const activeUrl = browser.activeUrl;
    if (isDdgUrl(activeUrl)) {
      const searchResponse = await humanBrowserSnapshotSearch(query, maxResults);
      if (searchResponse.challenge) {
        throw new Error(
          "Verification may still be required — finish the check in the browser tab, then try again.",
        );
      }
      await completeBrowserSession(sessionId, { searchResponse });
      return;
    }

    const snapshot = await humanBrowserSnapshotMarkdown(4000);
    await completeBrowserSession(sessionId, {
      searchResponse: {
        query,
        provider: "human_webview",
        results: [
          {
            title: snapshot.title || snapshot.url,
            url: snapshot.url,
            snippet: snapshot.markdown.slice(0, 400),
          },
        ],
        cached: false,
      },
    });
    return;
  }

  throw new Error("Browser verification resume requires the Medousa desktop app.");
}
