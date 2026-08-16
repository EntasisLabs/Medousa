const STYLE_ID = "medousa-selected-theme";

export function selectedThemeHref(skeletonName: string): string {
  return `/themes/${skeletonName}.css`;
}

export function applySelectedThemeStylesheet(skeletonName: string): void {
  if (typeof document === "undefined") return;
  const href = selectedThemeHref(skeletonName);
  let link = document.getElementById(STYLE_ID) as HTMLLinkElement | null;
  if (!link) {
    link = document.createElement("link");
    link.id = STYLE_ID;
    link.rel = "stylesheet";
    document.head.appendChild(link);
  }
  if (link.getAttribute("href") === href) return;
  link.href = href;
}
