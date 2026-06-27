/** Normalize URLs for bookmark dedupe and comparison. */

export function normalizeBrowserUrl(url: string): string {
  const trimmed = url.trim();
  if (!trimmed || trimmed === "about:blank") return trimmed;
  try {
    const parsed = new URL(trimmed);
    parsed.hash = "";
    const host = parsed.hostname.toLowerCase();
    let path = parsed.pathname.replace(/\/+$/, "") || "/";
    return `${parsed.protocol}//${host}${path}${parsed.search}`;
  } catch {
    return trimmed.toLowerCase();
  }
}

export function browserPageLabel(url: string, title?: string | null): string {
  const label = title?.trim();
  if (label) return label;
  if (!url || url === "about:blank") return "New tab";
  try {
    return new URL(url).hostname || url;
  } catch {
    return url.slice(0, 48);
  }
}
