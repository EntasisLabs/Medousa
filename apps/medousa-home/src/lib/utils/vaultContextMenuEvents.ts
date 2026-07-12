import {
  vaultContextMenu,
  type VaultContextTarget,
} from "$lib/stores/vaultContextMenu.svelte";
import { shouldUseMobileShell } from "$lib/platform";

const LONG_PRESS_MS = 520;
let suppressContextMenuClickUntil = 0;

export function shouldSuppressVaultContextMenuClick(): boolean {
  return Date.now() < suppressContextMenuClickUntil;
}

function markContextMenuOpened() {
  suppressContextMenuClickUntil = Date.now() + 350;
}

export function openVaultNoteContextMenu(
  path: string,
  clientX: number,
  clientY: number,
  selection?: { text: string; start?: number; end?: number } | null,
) {
  markContextMenuOpened();
  vaultContextMenu.showNote(path, clientX, clientY, selection);
}

export function openVaultFolderContextMenu(
  iconKey: string,
  label: string,
  clientX: number,
  clientY: number,
  spaceId?: string | null,
) {
  markContextMenuOpened();
  vaultContextMenu.showFolder(iconKey, label, clientX, clientY, spaceId);
}

export function openVaultAttachmentContextMenu(
  attachmentPath: string,
  notePath: string,
  clientX: number,
  clientY: number,
) {
  markContextMenuOpened();
  vaultContextMenu.showAttachment(attachmentPath, notePath, clientX, clientY);
}

export function handleVaultNoteContextMenuEvent(
  path: string,
  event: MouseEvent,
  selection?: { text: string; start?: number; end?: number } | null,
) {
  event.preventDefault();
  event.stopPropagation();
  openVaultNoteContextMenu(path, event.clientX, event.clientY, selection);
}

export function handleVaultFolderContextMenuEvent(
  iconKey: string,
  label: string,
  event: MouseEvent,
  spaceId?: string | null,
) {
  event.preventDefault();
  event.stopPropagation();
  openVaultFolderContextMenu(iconKey, label, event.clientX, event.clientY, spaceId);
}

function bindVaultContextTargetLongPress(
  node: HTMLElement,
  getTarget: () => VaultContextTarget | null,
): { destroy: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let startX = 0;
  let startY = 0;
  let pointerId: number | null = null;

  function clearTimer() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function openAt(clientX: number, clientY: number) {
    const target = getTarget();
    if (!target) return;
    if (target.kind === "note") {
      openVaultNoteContextMenu(target.path, clientX, clientY);
      return;
    }
    openVaultAttachmentContextMenu(target.path, target.notePath, clientX, clientY);
  }

  function onPointerDown(event: PointerEvent) {
    // Desktop uses right-click; long-press is a mobile affordance only.
    // Holding while scrolling on desktop must not open the context menu.
    if (!shouldUseMobileShell()) return;
    if (event.button !== 0) return;
    if (!getTarget()) return;
    pointerId = event.pointerId;
    startX = event.clientX;
    startY = event.clientY;
    clearTimer();
    timer = setTimeout(() => {
      timer = null;
      openAt(startX, startY);
    }, LONG_PRESS_MS);
  }

  function onPointerMove(event: PointerEvent) {
    if (pointerId !== event.pointerId || !timer) return;
    const dx = event.clientX - startX;
    const dy = event.clientY - startY;
    if (Math.hypot(dx, dy) > 10) {
      clearTimer();
    }
  }

  function onPointerUp(event: PointerEvent) {
    if (pointerId !== event.pointerId) return;
    pointerId = null;
    clearTimer();
  }

  node.addEventListener("pointerdown", onPointerDown);
  node.addEventListener("pointermove", onPointerMove);
  node.addEventListener("pointerup", onPointerUp);
  node.addEventListener("pointercancel", onPointerUp);

  return {
    destroy() {
      clearTimer();
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("pointercancel", onPointerUp);
    },
  };
}

export function bindVaultLongPress(
  node: HTMLElement,
  getPath: () => string | null,
): { destroy: () => void } {
  return bindVaultContextTargetLongPress(node, () => {
    const path = getPath();
    return path ? { kind: "note", path } : null;
  });
}

export function bindVaultAttachmentLongPress(
  node: HTMLElement,
  getTarget: () => { attachmentPath: string; notePath: string } | null,
): { destroy: () => void } {
  return bindVaultContextTargetLongPress(node, () => {
    const target = getTarget();
    if (!target) return null;
    return {
      kind: "attachment",
      path: target.attachmentPath,
      notePath: target.notePath,
    };
  });
}
