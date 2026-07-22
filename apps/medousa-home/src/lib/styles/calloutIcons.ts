/** Inline SVG icons for callout headers (stroke inherits accent color). */

export type CalloutIconKind =
  | "note"
  | "tip"
  | "success"
  | "warn"
  | "error"
  | "important";

const ATTRS =
  'xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"';

const ICONS: Record<CalloutIconKind, string> = {
  note: `<svg ${ATTRS}><circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/></svg>`,
  tip: `<svg ${ATTRS}><path d="M15 14c.2-1 .7-1.6 1.5-2.1A5.5 5.5 0 0 0 12 4a5.5 5.5 0 0 0-4.5 7.9c.8.5 1.3 1.1 1.5 2.1"/><path d="M9 18h6"/><path d="M10 22h4"/></svg>`,
  success: `<svg ${ATTRS}><circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/></svg>`,
  warn: `<svg ${ATTRS}><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/></svg>`,
  error: `<svg ${ATTRS}><circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/></svg>`,
  important: `<svg ${ATTRS}><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/><path d="M12 7v2"/><path d="M12 13h.01"/></svg>`,
};

export function calloutIconSvg(kind: CalloutIconKind): string {
  return ICONS[kind] ?? ICONS.note;
}

/** Map liquid / Obsidian kind → icon. */
export function calloutIconForTone(tone: string): CalloutIconKind {
  const t = tone.trim().toLowerCase();
  if (t === "tip") return "tip";
  if (t === "success") return "success";
  if (t === "warn" || t === "warning") return "warn";
  if (t === "error" || t === "danger" || t === "caution") return "error";
  if (t === "important") return "important";
  return "note";
}

/** Default header label when title is empty. */
export function calloutDefaultTitle(tone: string): string {
  const t = tone.trim().toLowerCase();
  if (t === "tip") return "Tip";
  if (t === "success") return "Success";
  if (t === "warn" || t === "warning") return "Warning";
  if (t === "error" || t === "danger" || t === "caution") return "Caution";
  if (t === "important") return "Important";
  return "Note";
}
