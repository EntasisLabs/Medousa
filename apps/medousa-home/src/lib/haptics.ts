/** Light haptic feedback via the Vibration API (mobile browsers / WebViews). */

type HapticKind = "light" | "medium" | "success" | "warning";

const PATTERNS: Record<HapticKind, number | number[]> = {
  light: 12,
  medium: 28,
  success: [18, 40, 18],
  warning: [36, 60, 36],
};

export function haptic(kind: HapticKind = "light"): void {
  if (typeof navigator === "undefined" || !("vibrate" in navigator)) return;
  try {
    navigator.vibrate(PATTERNS[kind]);
  } catch {
    // Unsupported or blocked — ignore.
  }
}
