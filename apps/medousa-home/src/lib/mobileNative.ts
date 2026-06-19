import {
  consumeWebWorkParam,
  parseDeepLink,
  type WorkDeepLink,
} from "$lib/deepLinks";
import { parsePairQrUrl } from "$lib/utils/pairingUrl";
import { isTauri } from "$lib/window";

export type OpenWorkHandler = (cardId: string) => void | Promise<void>;
export type OpenPairHandler = (pairUrl: string) => void;

let workHandler: OpenWorkHandler | null = null;
let pairHandler: OpenPairHandler | null = null;

async function dispatchWorkLink(link: WorkDeepLink) {
  if (!workHandler) return;
  await workHandler(link.cardId);
}

function handleUrls(urls: string[]) {
  for (const url of urls) {
    if (parsePairQrUrl(url)) {
      pairHandler?.(url);
      return;
    }
    const link = parseDeepLink(url);
    if (link?.kind === "work") {
      void dispatchWorkLink(link);
      return;
    }
  }
}

export function setPairDeepLinkHandler(handler: OpenPairHandler | null) {
  pairHandler = handler;
}

export function setWorkDeepLinkHandler(handler: OpenWorkHandler | null) {
  workHandler = handler;
}

export function initMobileNative(handler: OpenWorkHandler): () => void {
  setWorkDeepLinkHandler(handler);

  const cleanups: Array<() => void> = [];

  const webLink = consumeWebWorkParam();
  if (webLink) void dispatchWorkLink(webLink);

  if (isTauri()) {
    void (async () => {
      try {
        const { getCurrentDeepLinks, onDeepLinkOpen } = await import(
          "$lib/deepLinkTauri"
        );
        const initial = await getCurrentDeepLinks();
        if (initial?.length) handleUrls(initial);

        const unlisten = await onDeepLinkOpen((urls: string[]) =>
          handleUrls(urls),
        );
        cleanups.push(unlisten);
      } catch {
        // Plugin unavailable in Vite-only dev.
      }

      try {
        const { initNotificationRouting } = await import("$lib/notifications");
        const stop = await initNotificationRouting((cardId) => handler(cardId));
        if (stop) cleanups.push(stop);
      } catch {
        // Notifications optional.
      }
    })();
  }

  return () => {
    setWorkDeepLinkHandler(null);
    setPairDeepLinkHandler(null);
    for (const cleanup of cleanups) cleanup();
  };
}
