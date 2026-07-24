import {
  contentZoomPercent,
  resetContentZoom,
  stepContentZoom,
} from "$lib/config/contentZoom";
import { layout } from "$lib/stores/layout.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { toast } from "$lib/stores/toast.svelte";
import { summonViewToolbar } from "$lib/utils/railPopoverSummon";

const PREFIX_TIMEOUT_MS = 1200;

export type ShellPaneHotkeyHandlers = {
  onCheatSheet?: () => void;
};

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

function modalBlocksHotkeys(): boolean {
  if (typeof document === "undefined") return false;
  return Boolean(
    document.querySelector(
      '[role="dialog"][aria-modal="true"], [data-spotlight-open="true"]',
    ),
  );
}

/**
 * Attach window capture listeners for:
 * - Ctrl+B — toggle left rail
 * - Ctrl/Cmd+Shift+. — summon current view toolbar at cursor
 * - Ctrl+; then command — pane ops
 */
export function attachShellPaneHotkeys(handlers: ShellPaneHotkeyHandlers = {}): () => void {
  let prefixArmedUntil = 0;

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.defaultPrevented) return;
    if (modalBlocksHotkeys()) return;

    const ctrl = event.ctrlKey || event.metaKey;
    const key = event.key;
    const lower = key.length === 1 ? key.toLowerCase() : key;

    // Ctrl+B — left rail (VS Code / Cursor). Not a pane prefix.
    if (ctrl && !event.altKey && !event.shiftKey && lower === "b") {
      event.preventDefault();
      layout.toggleShellSidebarExpanded();
      return;
    }

    // Ctrl/Cmd+Shift+. — summon compact view toolbar at the cursor.
    if (ctrl && event.shiftKey && !event.altKey && (key === "." || key === ">")) {
      event.preventDefault();
      summonViewToolbar();
      return;
    }

    // Esc clears pane zoom outside the prefix chord.
    if (key === "Escape" && shellTabs.zoomedGroupId && Date.now() > prefixArmedUntil) {
      if (!isEditableTarget(event.target)) {
        event.preventDefault();
        shellTabs.clearZoom();
        return;
      }
    }

    // Arm prefix: Ctrl+;
    if (ctrl && !event.altKey && (key === ";" || key === ":")) {
      event.preventDefault();
      prefixArmedUntil = Date.now() + PREFIX_TIMEOUT_MS;
      return;
    }

    // Content zoom (notes / chats / scripts) — VS Code style. Not pane maximize.
    // Skip while Ctrl+; chord is armed so Ctrl+; - keeps "split down".
    if (
      ctrl &&
      !event.altKey &&
      !event.shiftKey &&
      Date.now() > prefixArmedUntil
    ) {
      if (key === "=" || key === "+" || key === "Add") {
        event.preventDefault();
        const zoom = stepContentZoom(1);
        toast.show(`Zoom ${contentZoomPercent(zoom)}`, { durationMs: 1200 });
        return;
      }
      if (key === "-" || key === "_" || key === "Subtract") {
        event.preventDefault();
        const zoom = stepContentZoom(-1);
        toast.show(`Zoom ${contentZoomPercent(zoom)}`, { durationMs: 1200 });
        return;
      }
      if (key === "0" || key === "Digit0") {
        event.preventDefault();
        const zoom = resetContentZoom();
        toast.show(`Zoom ${contentZoomPercent(zoom)}`, { durationMs: 1200 });
        return;
      }
    }

    if (Date.now() > prefixArmedUntil) return;

    // Second key of the chord — consume even in inputs.
    const handled = dispatchPrefixCommand(lower, key, handlers);
    if (handled) {
      event.preventDefault();
      prefixArmedUntil = 0;
    }
  };

  window.addEventListener("keydown", onKeyDown, true);
  return () => window.removeEventListener("keydown", onKeyDown, true);
}

/** Pure dispatch for tests. */
export function dispatchPrefixCommand(
  lower: string,
  rawKey: string,
  handlers: ShellPaneHotkeyHandlers = {},
): boolean {
  if (lower === "%" || rawKey === "|") {
    return shellTabs.splitActive("right");
  }
  if (lower === '"' || lower === "-" || rawKey === '"') {
    return shellTabs.splitActive("down");
  }
  if (lower === "h" || rawKey === "ArrowLeft") {
    shellTabs.focusDirection("left");
    return true;
  }
  if (lower === "l" || rawKey === "ArrowRight") {
    shellTabs.focusDirection("right");
    return true;
  }
  if (lower === "k" || rawKey === "ArrowUp") {
    shellTabs.focusDirection("up");
    return true;
  }
  if (lower === "j" || rawKey === "ArrowDown") {
    shellTabs.focusDirection("down");
    return true;
  }
  if (lower === "z") {
    shellTabs.zoomToggle();
    return true;
  }
  if (lower === "x") {
    return shellTabs.closeActiveGroup();
  }
  if (lower === "c") {
    // New chat tab in active pane — reuse current session seed / open surface chat.
    const sessionId = shellTabs.activeTab?.kind === "chat"
      ? shellTabs.activeTab.sessionId
      : null;
    if (sessionId) {
      shellTabs.openChat(sessionId, { activate: true });
    } else {
      shellTabs.openDestination("chat");
    }
    return true;
  }
  if (lower === "n") {
    shellTabs.nextTabInActiveGroup();
    return true;
  }
  if (lower === "p") {
    shellTabs.prevTabInActiveGroup();
    return true;
  }
  if (lower === "w") {
    shellTabs.flashTabs();
    return true;
  }
  if (lower === "?" || (rawKey === "?" && !isEditableTarget(null))) {
    handlers.onCheatSheet?.();
    return true;
  }
  // Virtual desktops 1–4 (occupied slots only — no-op if index empty).
  if (/^[1-4]$/.test(lower)) {
    void shellTabs.switchDesktopAt(Number(lower) - 1);
    return true;
  }
  if (rawKey === "Escape") {
    shellTabs.clearZoom();
    return true;
  }
  return false;
}
