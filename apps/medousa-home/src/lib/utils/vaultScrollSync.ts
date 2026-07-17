/** Bidirectional scroll sync for vault split edit/preview. */

const DEFAULT_LOCK_MS = 80;

export function scrollRatio(el: HTMLElement): number {
  const max = el.scrollHeight - el.clientHeight;
  if (max <= 0) return 0;
  return el.scrollTop / max;
}

export function applyScrollRatio(el: HTMLElement, ratio: number) {
  const max = el.scrollHeight - el.clientHeight;
  if (max <= 0) return;
  const clamped = Math.min(1, Math.max(0, ratio));
  el.scrollTop = clamped * max;
}

/** Short lock to avoid editor↔preview feedback loops. */
export function createVaultScrollSync(lockMs = DEFAULT_LOCK_MS) {
  let lockUntil = 0;

  function sync(source: HTMLElement, target: HTMLElement) {
    if (!source || !target || source === target) return;
    if (Date.now() < lockUntil) return;
    lockUntil = Date.now() + lockMs;
    applyScrollRatio(target, scrollRatio(source));
  }

  return { sync };
}
