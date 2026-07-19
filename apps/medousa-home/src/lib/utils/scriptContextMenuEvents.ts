import { scriptContextMenu } from "$lib/stores/scriptContextMenu.svelte";
import { shouldUseMobileShell } from "$lib/platform";

const LONG_PRESS_MS = 520;
let suppressContextMenuClickUntil = 0;

export function shouldSuppressScriptContextMenuClick(): boolean {
  return Date.now() < suppressContextMenuClickUntil;
}

function markContextMenuOpened() {
  suppressContextMenuClickUntil = Date.now() + 350;
}

export function openScriptContextMenu(
  scriptId: string,
  name: string,
  clientX: number,
  clientY: number,
) {
  markContextMenuOpened();
  scriptContextMenu.showAt(clientX, clientY, { scriptId, name });
}

export function handleScriptContextMenuEvent(
  scriptId: string,
  name: string,
  event: MouseEvent,
) {
  event.preventDefault();
  event.stopPropagation();
  openScriptContextMenu(scriptId, name, event.clientX, event.clientY);
}

/** Svelte action: long-press opens the scripts library context menu on mobile. */
export function bindScriptLongPress(
  node: HTMLElement,
  getTarget: () => { scriptId: string; name: string } | null,
): { destroy: () => void; update: (fn: typeof getTarget) => void } {
  let getter = getTarget;
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

  function onPointerDown(event: PointerEvent) {
    if (!shouldUseMobileShell()) return;
    if (event.button !== 0) return;
    const target = getter();
    if (!target) return;
    pointerId = event.pointerId;
    startX = event.clientX;
    startY = event.clientY;
    clearTimer();
    timer = setTimeout(() => {
      timer = null;
      const next = getter();
      if (!next) return;
      openScriptContextMenu(next.scriptId, next.name, startX, startY);
    }, LONG_PRESS_MS);
  }

  function onPointerMove(event: PointerEvent) {
    if (pointerId !== event.pointerId || !timer) return;
    if (Math.hypot(event.clientX - startX, event.clientY - startY) > 10) {
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
    update(fn) {
      getter = fn;
    },
    destroy() {
      clearTimer();
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("pointercancel", onPointerUp);
    },
  };
}
