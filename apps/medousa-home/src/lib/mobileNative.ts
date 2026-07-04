import {
  consumeWebWorkParam,
  parseDeepLink,
  type WorkDeepLink,
} from "$lib/deepLinks";
import { parsePairQrUrl } from "$lib/utils/pairingUrl";
import { isTauri } from "$lib/window";

export type OpenWorkHandler = (cardId: string) => void | Promise<void>;
export type OpenVaultNoteHandler = (notePath: string) => void | Promise<void>;
export type OpenPairHandler = (pairUrl: string) => void;

let workHandler: OpenWorkHandler | null = null;
let vaultHandler: OpenVaultNoteHandler | null = null;
/** Temporary override (e.g. onboarding wizard). */
let pairHandler: OpenPairHandler | null = null;
/** App-wide handler for medousa://pair/… after onboarding. */
let defaultPairHandler: OpenPairHandler | null = null;

async function dispatchWorkLink(link: WorkDeepLink) {
  if (!workHandler) return;
  await workHandler(link.cardId);
}

async function dispatchVaultLink(notePath: string) {
  if (!vaultHandler) return;
  await vaultHandler(notePath);
}

function dispatchPairLink(url: string) {
  const handler = pairHandler ?? defaultPairHandler;
  handler?.(url);
}

function handleUrls(urls: string[]) {
  for (const url of urls) {
    if (parsePairQrUrl(url)) {
      dispatchPairLink(url);
      return;
    }
    const link = parseDeepLink(url);
    if (link?.kind === "work") {
      void dispatchWorkLink(link);
      return;
    }
    if (link?.kind === "vault") {
      void dispatchVaultLink(link.notePath);
      return;
    }
  }
}

/** Override pair handling (wizard). Pass null to restore the default app handler. */
export function setPairDeepLinkHandler(handler: OpenPairHandler | null) {
  pairHandler = handler;
}

export function setVaultDeepLinkHandler(handler: OpenVaultNoteHandler | null) {
  vaultHandler = handler;
}

export function setWorkDeepLinkHandler(handler: OpenWorkHandler | null) {
  workHandler = handler;
}

export function initMobileNative(
  handler: OpenWorkHandler,
  vaultNoteHandler?: OpenVaultNoteHandler,
  options?: {
    onPairLink?: OpenPairHandler;
    onOpenPeer?: import("$lib/notifications").OpenPeerHandler;
  },
): () => void {
  setWorkDeepLinkHandler(handler);
  setVaultDeepLinkHandler(vaultNoteHandler ?? null);
  // Install default pair handler synchronously so cold-start deep links are not dropped.
  defaultPairHandler = options?.onPairLink ?? null;

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
        const stop = await initNotificationRouting(
          (cardId) => handler(cardId),
          options?.onOpenPeer,
        );
        if (stop) cleanups.push(stop);
      } catch {
        // Notifications optional.
      }

      try {
        const { initRemotePushHandlers } = await import("$lib/pushHandlers");
        const stop = await initRemotePushHandlers(handler, vaultNoteHandler ?? undefined);
        if (stop) cleanups.push(stop);
      } catch {
        // Remote push optional.
      }
    })();
  }

  return () => {
    setWorkDeepLinkHandler(null);
    setVaultDeepLinkHandler(null);
    setPairDeepLinkHandler(null);
    defaultPairHandler = null;
    for (const cleanup of cleanups) cleanup();
  };
}
