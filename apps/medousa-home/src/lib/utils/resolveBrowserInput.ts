/** Omnibox: distinguish URLs from search queries. */

export function looksLikeBrowserUrl(input: string): boolean {
  const trimmed = input.trim();
  if (!trimmed) return false;
  if (/^https?:\/\//i.test(trimmed)) return true;
  if (/^localhost(:\d+)?(\/|$)/i.test(trimmed)) return true;
  if (/^[\d.]+:\d+(\/|$)/.test(trimmed)) return true;
  if (/^[\d.]+(\/|$)/.test(trimmed)) return true;
  if (trimmed.includes(" ") && !trimmed.startsWith("http")) return false;
  if (/^[a-z0-9-]+(\.[a-z0-9-]+)+(\/.*)?$/i.test(trimmed)) return true;
  if (trimmed.includes(".") && !trimmed.includes(" ")) {
    try {
      new URL(`https://${trimmed}`);
      return true;
    } catch {
      return false;
    }
  }
  return false;
}

export function normalizeBrowserUrlInput(input: string): string {
  const trimmed = input.trim();
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) return trimmed;
  return `https://${trimmed}`;
}

export function duckDuckGoSearchUrl(query: string): string {
  return `https://duckduckgo.com/?q=${encodeURIComponent(query.trim())}`;
}
