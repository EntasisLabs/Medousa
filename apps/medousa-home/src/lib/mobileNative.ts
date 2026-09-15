import {
  consumeWebWorkParam,
  parseDeepLink,
  type WorkDeepLink,
} from "$lib/deepLinks";
import { parsePairQrUrl } from "$lib/utils/pairingUrl";
import { isTauri } from "$lib/window";
import { invoke } from "@tauri-apps/api/core";

export type OpenWorkHandler = (cardId: string) => void | Promise<void>;
export type OpenVaultNoteHandler = (notePath: string) => void | Promise<void>;
export type OpenPairHandler = (pairUrl: string) => void;
export type AskHandler = (requestId: string) => void | Promise<void>;

const SIRI_RECEIPT_WAIT_MS = 15_000;
const SIRI_RECEIPT_POLL_MS = 200;

let workHandler: OpenWorkHandler | null = null;
let vaultHandler: OpenVaultNoteHandler | null = null;
let askHandler: AskHandler | null = null;
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
    if (link?.kind === "ask") {
      void askHandler?.(link.requestId);
      return;
    }
    if (link?.kind === "vault") {
      void dispatchVaultLink(link.notePath);
      return;
    }
    if (link?.kind === "undertaking_location") {
      void import("$lib/utils/undertakingLocation").then(({ openUndertakingLocation }) =>
        openUndertakingLocation(link),
      );
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

export function setAskDeepLinkHandler(handler: AskHandler | null) {
  askHandler = handler;
}

export function initMobileNative(
  handler: OpenWorkHandler,
  vaultNoteHandler?: OpenVaultNoteHandler,
  options?: {
    onPairLink?: OpenPairHandler;
    onOpenPeer?: import("$lib/notifications").OpenPeerHandler;
    onOpenCalendar?: import("$lib/notifications").OpenCalendarHandler;
    onAsk?: AskHandler;
  },
): () => void {
  setWorkDeepLinkHandler(handler);
  setVaultDeepLinkHandler(vaultNoteHandler ?? null);
  // Install default pair handler synchronously so cold-start deep links are not dropped.
  defaultPairHandler = options?.onPairLink ?? null;
  setAskDeepLinkHandler(options?.onAsk ?? null);

  const cleanups: Array<() => void> = [];
  let active = true;

  const webLink = consumeWebWorkParam();
  if (webLink) void dispatchWorkLink(webLink);

  if (isTauri()) {
    void (async () => {
      let handledInitialAsk = false;
      try {
        const { getCurrentDeepLinks, onDeepLinkOpen } = await import(
          "$lib/deepLinkTauri"
        );
        const initial = await getCurrentDeepLinks();
        if (initial?.length) {
          handledInitialAsk = initial.some(
            (url) => parseDeepLink(url)?.kind === "ask",
          );
          handleUrls(initial);
        }

        const unlisten = await onDeepLinkOpen((urls: string[]) =>
          handleUrls(urls),
        );
        cleanups.push(unlisten);
      } catch {
        // Plugin unavailable in Vite-only dev.
      }

      // iOS can launch for an OpenURLIntent without retaining its URL through
      // webview startup. Recover only a just-created receipt after checking the
      // canonical deep-link path; normal native consumption still validates
      // and deletes the full payload.
      if (options?.onAsk && !handledInitialAsk) {
        void (async () => {
          const deadline = Date.now() + SIRI_RECEIPT_WAIT_MS;
          while (active && Date.now() < deadline) {
            try {
              const requestId = await invoke<string | null>(
                "siri_recent_pending_ask_id",
              );
              if (requestId) {
                await options.onAsk?.(requestId);
                break;
              }
            } catch {
              // Bridge unavailable; retrying cannot make it available.
              break;
            }
            await new Promise((resolve) =>
              setTimeout(resolve, SIRI_RECEIPT_POLL_MS),
            );
          }
        })();
      }

      try {
        const { initNotificationRouting } = await import("$lib/notifications");
        const stop = await initNotificationRouting(
          (cardId) => handler(cardId),
          options?.onOpenPeer,
          options?.onOpenCalendar,
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
    active = false;
    setWorkDeepLinkHandler(null);
    setVaultDeepLinkHandler(null);
    setPairDeepLinkHandler(null);
    setAskDeepLinkHandler(null);
    defaultPairHandler = null;
    for (const cleanup of cleanups) cleanup();
  };
}
