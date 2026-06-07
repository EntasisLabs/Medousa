/** Parse `medousa://work/{cardId}` and web dev fallbacks. */

export type WorkDeepLink = {
  kind: "work";
  cardId: string;
};

const WORK_PATH = /^\/work\/([^/?#]+)\/?$/i;

export function workDeepLinkUrl(cardId: string): string {
  return `medousa://work/${encodeURIComponent(cardId)}`;
}

export function parseDeepLink(raw: string): WorkDeepLink | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;

  try {
    if (trimmed.startsWith("medousa:")) {
      const url = new URL(trimmed);
      const host = url.hostname.toLowerCase();
      const pathId = url.pathname.replace(/^\/+/, "");
      if (host === "work" && pathId) {
        return { kind: "work", cardId: decodeURIComponent(pathId) };
      }
      const match = WORK_PATH.exec(url.pathname);
      if (match?.[1]) {
        return { kind: "work", cardId: decodeURIComponent(match[1]) };
      }
      return null;
    }

    const http = new URL(trimmed, "https://medousa.local");
    const match = WORK_PATH.exec(http.pathname);
    if (match?.[1]) {
      return { kind: "work", cardId: decodeURIComponent(match[1]) };
    }
  } catch {
    return null;
  }

  return null;
}

export function parseWebWorkParam(): WorkDeepLink | null {
  if (typeof window === "undefined") return null;
  const id = new URLSearchParams(window.location.search).get("work");
  if (!id?.trim()) return null;
  return { kind: "work", cardId: id.trim() };
}

export function consumeWebWorkParam(): WorkDeepLink | null {
  const link = parseWebWorkParam();
  if (!link || typeof window === "undefined") return link;
  const url = new URL(window.location.href);
  url.searchParams.delete("work");
  window.history.replaceState({}, "", url.pathname + url.search + url.hash);
  return link;
}
