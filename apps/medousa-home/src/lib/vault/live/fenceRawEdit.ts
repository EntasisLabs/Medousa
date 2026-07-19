/**
 * In-place raw fence edit for Live fenceBlock NodeViews.
 * Sized to the current rendered fence so scroll/place don’t jump.
 */

export type FenceRawEditHandles = {
  /** True while the textarea is mounted. */
  active: () => boolean;
  destroy: () => void;
};

export type FenceRawEditLockSize = {
  width: number;
  height: number;
};

export function measureFenceHost(host: HTMLElement): FenceRawEditLockSize {
  const rect = host.getBoundingClientRect();
  return {
    width: Math.max(rect.width || host.offsetWidth || 240, 160),
    height: Math.max(rect.height || host.offsetHeight || 120, 96),
  };
}

export function mountFenceRawEdit(
  host: HTMLElement,
  raw: string,
  options: {
    onCommit: (nextRaw: string) => void;
    onCancel: () => void;
    /** Keep the editor box at the rendered fence size. */
    lockSize?: FenceRawEditLockSize;
    /** Restore page scroll after commit/cancel (avoids jump). */
    scrollTop?: number;
    scrollParent?: HTMLElement | null;
  },
): FenceRawEditHandles {
  let settled = false;
  const lock = options.lockSize ?? measureFenceHost(host);

  const shell = document.createElement("div");
  shell.className = "vault-live-fence-raw-shell";
  shell.style.width = `${Math.round(lock.width)}px`;
  shell.style.height = `${Math.round(lock.height)}px`;

  const ta = document.createElement("textarea");
  ta.className = "vault-live-fence-raw";
  ta.value = raw;
  ta.spellcheck = false;
  ta.setAttribute("aria-label", "Edit code fence");
  ta.title = "Mod+Enter to apply · Esc to cancel";

  shell.append(ta);
  host.replaceChildren(shell);

  const restoreScroll = () => {
    const parent = options.scrollParent;
    if (parent && typeof options.scrollTop === "number") {
      parent.scrollTop = options.scrollTop;
    }
  };

  const settle = (commit: boolean) => {
    if (settled) return;
    settled = true;
    ta.removeEventListener("blur", onBlur);
    if (commit) {
      let next = ta.value.replace(/\r\n/g, "\n");
      if (!next.endsWith("\n")) next += "\n";
      options.onCommit(next);
    } else {
      options.onCancel();
    }
    requestAnimationFrame(restoreScroll);
  };

  const onBlur = () => settle(true);

  ta.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      settle(false);
      return;
    }
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      e.stopPropagation();
      settle(true);
    }
  });
  ta.addEventListener("blur", onBlur);

  requestAnimationFrame(() => {
    restoreScroll();
    ta.focus();
    // Keep caret near the start of the visible area — not the end of a long fence.
    ta.setSelectionRange(0, 0);
    ta.scrollTop = 0;
  });

  return {
    active: () => !settled,
    destroy: () => {
      settled = true;
      ta.removeEventListener("blur", onBlur);
    },
  };
}
