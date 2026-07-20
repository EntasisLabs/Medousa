/**
 * Vault editor default hotkeys (no remapping UI).
 * Call from VaultEditor keydown; Live TipTap keeps its own format keys.
 */

export type VaultHotkeyAction =
  | "save"
  | "find"
  | "togglePlane"
  | "exportPdf"
  | "toggleBoard"
  | "enterEdit"
  | "enterPreview";

export function isPlainTextEditingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if (target.isContentEditable) return true;
  if (target.closest(".cm-editor")) return true;
  if (target.closest(".vault-live-fence-raw")) return true;
  return false;
}

/** True when the event is a vault chrome shortcut (not character typing). */
export function matchVaultHotkey(event: KeyboardEvent): VaultHotkeyAction | null {
  const mod = event.metaKey || event.ctrlKey;
  const key = event.key.toLowerCase();

  if (mod && !event.altKey && key === "s" && !event.shiftKey) return "save";
  if (mod && !event.altKey && key === "f" && !event.shiftKey) return "find";
  if (mod && event.shiftKey && !event.altKey && key === "e") return "togglePlane";
  if (mod && event.shiftKey && !event.altKey && key === "p") return "exportPdf";
  if (mod && event.shiftKey && !event.altKey && key === "b") return "toggleBoard";

  if (!mod && !event.altKey && !event.shiftKey && key === "e") return "enterEdit";
  if (!mod && !event.altKey && key === "escape") return "enterPreview";

  return null;
}
