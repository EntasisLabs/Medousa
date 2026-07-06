/** Append-only browsing history ring for omnibox suggestions. */

import { normalizeBrowserUrl } from "$lib/utils/browserUrl";

export type BrowserHistoryEntry = {
  url: string;
  title: string;
  visitedAt: string;
};

const STORAGE_KEY = "medousa-browser-history";
const MAX_ENTRIES = 200;

function loadHistory(): BrowserHistoryEntry[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as BrowserHistoryEntry[];
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (entry) =>
        typeof entry.url === "string" &&
        entry.url !== "about:blank" &&
        typeof entry.visitedAt === "string",
    );
  } catch {
    return [];
  }
}

function persistHistory(entries: BrowserHistoryEntry[]) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(entries.slice(0, MAX_ENTRIES)));
}

export class BrowserHistoryStore {
  entries = $state<BrowserHistoryEntry[]>(loadHistory());

  record(url: string, title?: string | null) {
    const trimmed = url.trim();
    if (!trimmed || trimmed === "about:blank") return;
    const norm = normalizeBrowserUrl(trimmed);
    const label = title?.trim() || trimmed;
    const next: BrowserHistoryEntry = {
      url: trimmed,
      title: label,
      visitedAt: new Date().toISOString(),
    };
    this.entries = [
      next,
      ...this.entries.filter((entry) => normalizeBrowserUrl(entry.url) !== norm),
    ].slice(0, MAX_ENTRIES);
    persistHistory(this.entries);
  }

  recent(limit = 8): BrowserHistoryEntry[] {
    return this.entries.slice(0, limit);
  }

  search(query: string, limit = 8): BrowserHistoryEntry[] {
    const trimmed = query.trim().toLowerCase();
    if (!trimmed) return this.recent(limit);
    return this.entries
      .filter((entry) => {
        const haystack = `${entry.title} ${entry.url}`.toLowerCase();
        return haystack.includes(trimmed);
      })
      .slice(0, limit);
  }
}

export const browserHistory = new BrowserHistoryStore();
