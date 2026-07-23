/** Cross-component Work hub chrome actions (rail toolbar → panel). */

export const WORK_FOCUS_ASK_EVENT = "medousa-work-focus-ask";
export const WORK_OPEN_TRAY_EVENT = "medousa-work-open-tray";

export type WorkTrayId = "settled" | "failed" | "stopped" | "stuck";

export function dispatchWorkFocusAsk() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(WORK_FOCUS_ASK_EVENT));
}

export function dispatchWorkOpenTray(tray: WorkTrayId) {
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent(WORK_OPEN_TRAY_EVENT, { detail: { tray } }),
  );
}
