const STYLE_ID = "medousa-selected-theme";
const PENDING_THEME = "data-medousa-pending-theme";

export function selectedThemeHref(skeletonName: string): string {
  return `/themes/${skeletonName}.css`;
}

export function applySelectedThemeStylesheet(skeletonName: string): void {
  if (typeof document === "undefined") return;
  const href = selectedThemeHref(skeletonName);
  const current = document.getElementById(STYLE_ID) as HTMLLinkElement | null;
  if (current?.getAttribute("href") === href) return;
  document.querySelector<HTMLLinkElement>(`link[${PENDING_THEME}]`)?.remove();

  const next = document.createElement("link");
  next.rel = "stylesheet";
  next.href = href;
  if (!current) {
    next.id = STYLE_ID;
    document.head.appendChild(next);
    return;
  }

  next.setAttribute(PENDING_THEME, "true");
  next.addEventListener("load", () => {
    if (!next.isConnected) return;
    current.remove();
    next.removeAttribute(PENDING_THEME);
    next.id = STYLE_ID;
  }, { once: true });
  next.addEventListener("error", () => next.remove(), { once: true });
  document.head.appendChild(next);
}
