/** Parse app-owned `medousa://` navigation and callback links. */

export type WorkDeepLink = {
  kind: "work";
  cardId: string;
};

export type VaultDeepLink = {
  kind: "vault";
  notePath: string;
};

export type UndertakingLocationDeepLink = {
  kind: "undertaking_location";
  workId: string;
  path: string;
  line: number | null;
  entityId: string | null;
};

export type DeepLink =
  | WorkDeepLink
  | VaultDeepLink
  | UndertakingLocationDeepLink;

const WORK_PATH = /^\/work\/([^/?#]+)\/?$/i;

function isRepositoryRelativePath(path: string): boolean {
  const normalized = path.replaceAll("\\", "/");
  return (
    !!normalized &&
    !normalized.startsWith("/") &&
    !/^[a-z]:\//i.test(normalized) &&
    !normalized.split("/").includes("..")
  );
}

export function workDeepLinkUrl(cardId: string): string {
  return `medousa://work/${encodeURIComponent(cardId)}`;
}

export function vaultDeepLinkUrl(notePath: string): string {
  return `medousa://vault/${encodeURIComponent(notePath.replace(/^\/+/, ""))}`;
}

export function undertakingLocationDeepLinkUrl(input: {
  workId: string;
  path: string;
  line?: number | null;
  entityId?: string | null;
}): string {
  const url = new URL(`medousa://undertaking/${encodeURIComponent(input.workId)}/location`);
  url.searchParams.set("path", input.path);
  if (input.line != null) url.searchParams.set("line", String(input.line));
  if (input.entityId) url.searchParams.set("entity", input.entityId);
  return url.toString();
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
      if (host === "undertaking") {
        const segments = url.pathname.split("/").filter(Boolean);
        const workId = segments[0] ? decodeURIComponent(segments[0]) : "";
        const path = url.searchParams.get("path")?.trim() ?? "";
        const rawLine = Number(url.searchParams.get("line"));
        if (!workId || segments[1] !== "location" || !isRepositoryRelativePath(path)) {
          return null;
        }
        return {
          kind: "undertaking_location",
          workId,
          path,
          line: Number.isInteger(rawLine) && rawLine > 0 ? rawLine : null,
          entityId: url.searchParams.get("entity")?.trim() || null,
        };
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
