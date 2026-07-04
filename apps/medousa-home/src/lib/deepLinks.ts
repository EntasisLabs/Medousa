/** Parse `medousa://work/{cardId}` and `medousa://vault/{notePath}` deeplinks. */

export type WorkDeepLink = {
  kind: "work";
  cardId: string;
};

export type VaultDeepLink = {
  kind: "vault";
  notePath: string;
};

export type DeepLink = WorkDeepLink | VaultDeepLink;

const WORK_PATH = /^\/work\/([^/?#]+)\/?$/i;

export function workDeepLinkUrl(cardId: string): string {
  return `medousa://work/${encodeURIComponent(cardId)}`;
}

export function vaultDeepLinkUrl(notePath: string): string {
  return `medousa://vault/${encodeURIComponent(notePath.replace(/^\/+/, ""))}`;
}

export function parseDeepLink(raw: string): DeepLink | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;

  try {
    if (trimmed.startsWith("medousa:")) {
      const url = new URL(trimmed);
      const host = url.hostname.toLowerCase();
      const pathSegment = url.pathname.replace(/^\/+/, "");
      if (host === "work" && pathSegment) {
        return { kind: "work", cardId: decodeURIComponent(pathSegment) };
      }
      if (host === "vault") {
        const rawPath = trimmed.replace(/^medousa:\/\/vault\/?/i, "");
        if (!rawPath || rawPath.includes("..")) return null;
        const notePath = decodeURIComponent(rawPath);
        if (notePath && !notePath.includes("..") && !notePath.startsWith("/")) {
          return { kind: "vault", notePath };
        }
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
