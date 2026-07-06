/** Favicon + tab label helpers for browser chrome. */

export function hostnameFromUrl(url: string): string {
  try {
    return new URL(url).hostname || url;
  } catch {
    return url;
  }
}

export function faviconUrlForSite(url: string, size = 32): string {
  const host = hostnameFromUrl(url);
  if (!host || host === "about:blank") return "";
  return `https://www.google.com/s2/favicons?domain=${encodeURIComponent(host)}&sz=${size}`;
}

export function tabDisplayLabel(title: string | undefined | null, url: string): string {
  const trimmed = title?.trim();
  if (trimmed && trimmed.toLowerCase() !== "new tab") return trimmed;
  const host = hostnameFromUrl(url);
  if (host && host !== "about:blank") return host;
  return trimmed || "New tab";
}
