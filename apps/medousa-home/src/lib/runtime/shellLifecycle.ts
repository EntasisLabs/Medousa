import { initMobileNative } from "$lib/mobileNative";
import { startPeerMessageNotificationPolling } from "$lib/peerNotifications";
import { layout } from "$lib/stores/layout.svelte";
import { toast } from "$lib/stores/toast.svelte";
import { wizard } from "$lib/stores/wizard.svelte";
import { workshops } from "$lib/stores/workshops.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { workAskDock } from "$lib/stores/workAskDock.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { setUndertakingGroupIdPort } from "$lib/stores/undertakings.svelte";
import { setActiveWorkshopKindPort } from "$lib/utils/workshopLocality";
import { setWorkshopReconnectPort } from "./workshopReconnectPort";
import { reconnectWorkshop } from "$lib/workshopConnection";
import {
  applyNativeMobileShellLayout,
  isTauri,
  isTauriMobilePlatform,
  watchMobileViewport,
} from "$lib/platform";
import { handoffBrowserShell } from "$lib/utils/browserShellHandoff";
import { attachAgentBrowserCoord } from "$lib/utils/agentBrowserCoord";
import { WORK_FOCUS_ASK_EVENT } from "$lib/utils/workChromeEvents";
import { humanBrowserSetMobileShellActive } from "$lib/humanBrowser";
import { bindRootResource, recordRootResource } from "./rootResources";
import {
  openCalendarEvent,
  openPeerThread,
  openVaultNote,
  openWorkCard,
} from "./shellUseCases";

/** Named shell owner for AppShell onMount pollers/listeners. */
export function startShellRootResources(): () => void {
  setUndertakingGroupIdPort(() => shellTabs.activeGroupId);
  setActiveWorkshopKindPort(() => workshops.activeWorkshop?.kind);
  setWorkshopReconnectPort((onHealthChange) =>
    reconnectWorkshop(onHealthChange ?? (() => {})),
  );
  commandSpotlight.closeSpotlight();
  document.querySelectorAll(".command-spotlight-backdrop").forEach((node) => {
    node.closest(".body-portal-host")?.remove() ?? node.remove();
  });

  void wizard.bootstrap();
  const stopWizard = recordRootResource("wizard-bootstrap");
  const stopViewport = bindRootResource("viewport-tracking", layout.attachViewportTracking());
  if (isTauri()) {
    void humanBrowserSetMobileShellActive(layout.isMobile);
  }
  const stopNativeLayout = bindRootResource(
    "native-mobile-layout",
    applyNativeMobileShellLayout(),
  );
  const stopMobileViewport = bindRootResource(
    "mobile-viewport",
    isTauriMobilePlatform()
      ? () => {
          layout.setMobile(true);
        }
      : watchMobileViewport((mobile) => {
          const wasMobile = layout.isMobile;
          layout.setMobile(mobile);
          if (wasMobile !== mobile) {
            handoffBrowserShell(mobile);
          }
        }),
  );
  const stopNative = bindRootResource(
    "mobile-native",
    initMobileNative(openWorkCard, openVaultNote, {
      onPairLink: (pairUrl) => {
        void workshops
          .joinFromPairLink(pairUrl)
          .then((result) => {
            toast.show(`Connected to ${result.workshopPeerName}`);
            if (result.workshopId) {
              void workshops.selectWorkshop(result.workshopId);
            }
          })
          .catch((err) => {
            toast.show(err instanceof Error ? err.message : String(err), {
              durationMs: 4500,
            });
          });
      },
      onOpenPeer: openPeerThread,
      onOpenCalendar: openCalendarEvent,
    }),
  );
  const stopPeerNotifications = bindRootResource(
    "peer-message-notifications",
    startPeerMessageNotificationPolling(),
  );
  const stopAgentBrowserCoord = bindRootResource(
    "agent-browser-coord",
    attachAgentBrowserCoord(),
  );

  const onKeydown = (event: KeyboardEvent) => {
    if (layout.isMobile) return;

    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      commandSpotlight.toggleSpotlight();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "p") {
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);
      if (typing) return;
      if (target?.closest?.("[data-vault-editor]")) return;
      event.preventDefault();
      commandSpotlight.openCommandPalette();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "o") {
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);
      if (typing) return;
      event.preventDefault();
      commandSpotlight.openNotes();
    }
  };
  window.addEventListener("keydown", onKeydown);
  const stopHotkeys = bindRootResource("command-spotlight-hotkeys", () => {
    window.removeEventListener("keydown", onKeydown);
  });

  const onFocusAsk = () => {
    if (layout.isMobile) return;
    const trigger =
      document.querySelector<HTMLElement>('[data-work-ask-trigger="true"]') ?? null;
    workAskDock.openDock(trigger);
  };
  window.addEventListener(WORK_FOCUS_ASK_EVENT, onFocusAsk);
  const stopWorkAskFocus = bindRootResource("work-ask-focus", () => {
    window.removeEventListener(WORK_FOCUS_ASK_EVENT, onFocusAsk);
  });

  return () => {
    stopWizard();
    stopNativeLayout();
    stopViewport();
    stopMobileViewport();
    stopNative();
    stopPeerNotifications();
    stopAgentBrowserCoord();
    stopHotkeys();
    stopWorkAskFocus();
    setActiveWorkshopKindPort(() => undefined);
    setWorkshopReconnectPort(null);
  };
}
