import { haptic } from "$lib/haptics";
import { chat } from "$lib/stores/chat.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import { MOBILE_TABS, type MobileTab } from "$lib/types/mobile";

export const MOBILE_TAB_ORDER: MobileTab[] = MOBILE_TABS.map((tab) => tab.id);

type MobileBackHandler = () => boolean;

const mobileBackHandlers: MobileBackHandler[] = [];

/** Panels register nested back (e.g. Context detail). Last registered wins first. */
export function registerMobileBackHandler(handler: MobileBackHandler): () => void {
  mobileBackHandlers.push(handler);
  return () => {
    const index = mobileBackHandlers.indexOf(handler);
    if (index >= 0) mobileBackHandlers.splice(index, 1);
  };
}

export function mobileOverlaysOpen(): boolean {
  return (
    layout.activitySheetOpen ||
    layout.askSheetOpen ||
    layout.sessionDrawerOpen ||
    layout.identityDrawerOpen
  );
}

export function tryMobileBackNavigation(): boolean {
  if (layout.activitySheetOpen) {
    layout.setActivitySheetOpen(false);
    return true;
  }
  if (layout.askSheetOpen) {
    layout.setAskSheetOpen(false);
    return true;
  }
  if (layout.sessionDrawerOpen) {
    layout.setSessionDrawerOpen(false);
    return true;
  }
  if (layout.identityDrawerOpen) {
    layout.setIdentityDrawerOpen(false);
    return true;
  }
  for (let index = mobileBackHandlers.length - 1; index >= 0; index -= 1) {
    if (mobileBackHandlers[index]()) return true;
  }
  if (layout.mobileTab === "work" && workspace.selectedCardId) {
    workspace.clearSelection();
    return true;
  }
  if (layout.mobileTab === "you" && layout.youDestination !== "hub") {
    layout.backToYouHub();
    return true;
  }
  return false;
}

export function adjacentMobileTab(current: MobileTab, direction: 1 | -1): MobileTab | null {
  const index = MOBILE_TAB_ORDER.indexOf(current);
  if (index < 0) return null;
  const next = index + direction;
  if (next < 0 || next >= MOBILE_TAB_ORDER.length) return null;
  return MOBILE_TAB_ORDER[next];
}

export function switchMobileTab(tab: MobileTab): void {
  haptic("light");
  layout.setActivitySheetOpen(false);
  layout.setAskSheetOpen(false);
  if (tab !== "chat") {
    layout.setSessionDrawerOpen(false);
    layout.setIdentityDrawerOpen(false);
  }
  layout.setMobileTab(tab);
  if (tab === "you") {
    layout.backToYouHub();
  } else if (tab === "chat") {
    layout.backToYouHub();
    void chat.refreshSessions();
    void chat.ensureSessionHydrated();
  }
}

export function stepMobileTab(direction: 1 | -1): boolean {
  const next = adjacentMobileTab(layout.mobileTab, direction);
  if (!next) return false;
  switchMobileTab(next);
  return true;
}
