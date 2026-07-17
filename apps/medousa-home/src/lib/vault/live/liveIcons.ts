/** Tiny lucide-style SVGs for Live NodeView chrome (no Svelte). */

function svg(paths: string, className = "vault-live-icon"): string {
  return `<svg class="${className}" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}

export const LIVE_ICON_OPEN = svg(
  `<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>`,
);

export const LIVE_ICON_DETACH = svg(
  `<path d="m18.84 12.25 1.72-1.71h-.02a5.004 5.004 0 0 0-.12-7.07 5.006 5.006 0 0 0-6.95 0l-1.72 1.71"/><path d="m5.17 11.75-1.71 1.71a5.004 5.004 0 0 0 .12 7.07 5.006 5.006 0 0 0 6.95 0l1.71-1.71"/><line x1="8" x2="8" y1="2" y2="8"/><line x1="2" x2="8" y1="8" y2="8"/><line x1="16" x2="22" y1="16" y2="16"/><line x1="16" x2="16" y1="16" y2="22"/>`,
);

export const LIVE_ICON_CHEVRON_DOWN = svg(
  `<path d="m6 9 6 6 6-6"/>`,
);

export const LIVE_ICON_CHEVRON_UP = svg(
  `<path d="m18 15-6-6-6 6"/>`,
);
