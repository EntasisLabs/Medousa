import { initMobileNative } from "$lib/mobileNative";
import { startPeerMessageNotificationPolling } from "$lib/peerNotifications";
import { layout } from "$lib/runtime/layout.svelte";
import { toast } from "$lib/runtime/toast.svelte";
import { wizard } from "$lib/stores/wizard.svelte";
import { workshops } from "$lib/stores/workshops.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { workAskDock } from "$lib/stores/workAskDock.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { setUndertakingGroupIdPort } from "./undertakingGroupPort";
import { isVaultDirty } from "./vaultDirtySnapshot";
import { setActiveWorkshopKindPort } from "$lib/utils/workshopLocality";
import { setWorkshopReconnectPort } from "./workshopReconnectPort";
import { setArtifactSessionTitlePort } from "./artifactSessionTitlePort";
import { setProfileSwitchPorts } from "./profileSwitchPorts";
import { setWorkshopSwitchPorts } from "./workshopSwitchPorts";
import { setWorkspaceChatPort } from "./workspaceChatPort";
import { setWorkCardHideAfterHoursPort } from "./workCardHideAfterHoursPort";
import { reconnectWorkshop } from "$lib/workshopConnection";
import {
  applyNativeMobileShellLayout,
  isTauri,
  isTauriMobilePlatform,
  watchMobileViewport,
} from "$lib/platform";
import { handoffBrowserShell } from "$lib/utils/browserShellHandoff";
import { WORK_FOCUS_ASK_EVENT } from "$lib/utils/workChromeEvents";
import { humanBrowserSetMobileShellActive } from "$lib/humanBrowser";
import { bindRootResource } from "./rootResources";
import { eventMatchesCommandChord } from "$lib/commands/commandBindings";
import {
  openCalendarEvent,
  openPeerThread,
  openVaultNote,
  openWorkCard,
} from "./shellUseCases";
import { bindAllFeaturePorts, unbindAllFeaturePorts } from "./bindFeaturePorts";
import { disposeFeature } from "./features/loader";
import { disposeDestinationFeatures } from "./features/disposeDestinations";

/** Named shell owner for AppShell onMount pollers/listeners. */
export function startShellRootResources(): () => void {
  bindAllFeaturePorts();
  setUndertakingGroupIdPort(() => shellTabs.activeGroupId);
  setActiveWorkshopKindPort(() => workshops.activeWorkshop?.kind);
  setWorkshopReconnectPort((onHealthChange) =>
    reconnectWorkshop(onHealthChange ?? (() => {})),
  );
  setArtifactSessionTitlePort((sessionId) => {
    const match = chat.sessions.find((session) => session.session_id === sessionId);
    return match?.display_name?.trim() || sessionId;
  });
  setProfileSwitchPorts({
    hasConversation: () => chat.messages.length > 0,
    refreshSessions: () => chat.refreshSessions({ force: true }),
    refreshIdentity: async (userId) => {
      const { identity } = await import("$lib/stores/identity.svelte");
      await identity.refresh({ relationshipLimit: 8, userId });
    },
  });
  setWorkshopSwitchPorts({
    vaultDirty: isVaultDirty,
    flushVaultBeforeLeave: async () => {
      const { vault } = await import("$lib/stores/vault.svelte");
      return vault.flushBeforeLeave();
    },
    hasLiveInteractiveTurn: () => chat.hasLiveInteractiveTurn(),
    chatSessionId: () => chat.sessionId,
    chatHasSession: (sessionId) =>
      chat.sessions.some((session) => session.session_id === sessionId),
    switchChatSession: (sessionId) => chat.switchSession(sessionId),
    setDaemonUrl: (url) => {
      settings.daemonUrl = url;
    },
    setColorTheme: (themeId, options) => settings.setColorTheme(themeId, options),
    applyTheme: () => settings.applyTheme(),
  });
  setWorkspaceChatPort({
    noteAskTurnSettled: (cardId) => chat.noteAskTurnSettled(cardId),
    hasPendingBudgetApproval: (cardId) => chat.hasPendingBudgetApproval(cardId),
    noteBackgroundSettled: () => chat.noteBackgroundSettled(),
    noteBudgetResolved: (cardId) => chat.noteBudgetResolved(cardId),
    syncWorkerLaneFromCards: (cards, details) =>
      chat.syncWorkerLaneFromCards(cards, details),
    pendingWorkerSynthesisIds: () => chat.pendingWorkerSynthesisIds(),
    recoverPendingWorkerSyntheses: (cards, details) =>
      chat.recoverPendingWorkerSyntheses(cards, details),
    onWorkerCardDetail: (detail, column, previousColumn) =>
      chat.onWorkerCardDetail(detail, column, previousColumn),
    hasPendingWorkerSynthesis: (cardOrWorkId) =>
      chat.hasPendingWorkerSynthesis(cardOrWorkId),
    noteWorkerSynthesisFailure: (workId, errorLine) =>
      chat.noteWorkerSynthesisFailure(workId, errorLine),
  });
  setWorkCardHideAfterHoursPort(() => settings.workCardHideAfterHours);
  commandSpotlight.closeSpotlight();
  document.querySelectorAll(".command-spotlight-backdrop").forEach((node) => {
    node.closest(".body-portal-host")?.remove() ?? node.remove();
  });

  const wizardBootstrap = new AbortController();
  void wizard.bootstrap(wizardBootstrap.signal);
  const stopWizard = bindRootResource("wizard-bootstrap", () => {
    wizardBootstrap.abort("teardown");
  });
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
            void disposeFeature(
              mobile ? "shell-desktop" : "shell-mobile",
              "platform-switch",
            );
            void disposeDestinationFeatures("platform-switch");
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
  const onKeydown = (event: KeyboardEvent) => {
    if (layout.isMobile) return;

    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      commandSpotlight.toggleSpotlight();
      return;
    }
    if (eventMatchesCommandChord(event, "workbench.action.showCommands")) {
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
    stopHotkeys();
    stopWorkAskFocus();
    setActiveWorkshopKindPort(() => undefined);
    setWorkshopReconnectPort(null);
    setArtifactSessionTitlePort(null);
    setProfileSwitchPorts(null);
    setWorkshopSwitchPorts(null);
    setWorkspaceChatPort(null);
    setWorkCardHideAfterHoursPort(null);
    unbindAllFeaturePorts();
    void disposeDestinationFeatures("teardown");
  };
}
