/** Local quick-access browser bookmarks (star list). */

import { normalizeBrowserUrl } from "$lib/utils/browserUrl";

export type BrowserBookmark = {
  url: string;
  title: string;
  savedAt: string;
  vaultPath?: string;
};

const STORAGE_KEY = "medousa-browser-bookmarks";

function loadBookmarks(): BrowserBookmark[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as BrowserBookmark[];
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (entry) =>
        typeof entry.url === "string" &&
        typeof entry.title === "string" &&
        typeof entry.savedAt === "string",
    );
  } catch {
    return [];
  }
}

function persistBookmarks(bookmarks: BrowserBookmark[]) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(bookmarks));
}

export class BrowserBookmarksStore {
  bookmarks = $state<BrowserBookmark[]>(loadBookmarks());

  isBookmarked(url: string): boolean {
    const norm = normalizeBrowserUrl(url);
    return this.bookmarks.some((entry) => normalizeBrowserUrl(entry.url) === norm);
  }

  list(): BrowserBookmark[] {
    return [...this.bookmarks].sort(
      (a, b) => Date.parse(b.savedAt) - Date.parse(a.savedAt),
    );
  }

  add(url: string, title: string, vaultPath?: string) {
    const norm = normalizeBrowserUrl(url);
    const existing = this.bookmarks.find(
      (entry) => normalizeBrowserUrl(entry.url) === norm,
    );
    if (existing) {
      this.bookmarks = this.bookmarks.map((entry) =>
        normalizeBrowserUrl(entry.url) === norm
          ? {
              ...entry,
              title: title.trim() || entry.title,
              vaultPath: vaultPath ?? entry.vaultPath,
            }
          : entry,
      );
    } else {
      this.bookmarks = [
        {
          url,
          title: title.trim() || url,
          savedAt: new Date().toISOString(),
          vaultPath,
        },
        ...this.bookmarks,
      ];
    }
    persistBookmarks(this.bookmarks);
  }

  /** Returns true when bookmark was added, false when removed. */
  toggle(url: string, title: string): boolean {
    if (this.isBookmarked(url)) {
      this.remove(url);
      return false;
    }
    this.add(url, title);
    return true;
  }

  remove(url: string) {
    const norm = normalizeBrowserUrl(url);
    this.bookmarks = this.bookmarks.filter(
      (entry) => normalizeBrowserUrl(entry.url) !== norm,
    );
    persistBookmarks(this.bookmarks);
  }

  linkVaultPath(url: string, vaultPath: string) {
    const norm = normalizeBrowserUrl(url);
    this.bookmarks = this.bookmarks.map((entry) =>
      normalizeBrowserUrl(entry.url) === norm ? { ...entry, vaultPath } : entry,
    );
    persistBookmarks(this.bookmarks);
  }

  findByUrl(url: string): BrowserBookmark | null {
    const norm = normalizeBrowserUrl(url);
    return (
      this.bookmarks.find((entry) => normalizeBrowserUrl(entry.url) === norm) ??
      null
    );
  }
}

export const browserBookmarks = new BrowserBookmarksStore();
