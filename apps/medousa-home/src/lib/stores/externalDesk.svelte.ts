import type { ExternalDirectoryPage, ExternalFileEntry, LibrarySidebarMode, PinnedRoot } from "$lib/types/externalDesk";
import {
  pickExternalFolder,
  rootLabelFromPath,
  listExternalDirectory,
} from "$lib/utils/externalDeskApi";
import { guessMimeFromPath } from "$lib/utils/vaultAttachments";
import { randomUuid } from "$lib/utils/randomUuid";
import {
  isCoLocatedWorkshop,
  vaultPinFolderRemoteHint,
} from "$lib/utils/workshopLocality";

const PINNED_ROOTS_KEY = "medousa-home-pinned-roots";
const SIDEBAR_MODE_KEY = "medousa-home-library-sidebar-mode";
const MAX_PINNED_ROOTS = 5;

export interface DirectoryState extends ExternalDirectoryPage {
  cursor: string | null;
  previous: (string | null)[];
  loading: boolean;
  error: string | null;
}

function inside(path: string, parent: string) {
  const normalize = (value: string) => value.replace(/\\/g, "/").replace(/\/+$/, "");
  return normalize(path) === normalize(parent) || normalize(path).startsWith(`${normalize(parent)}/`);
}

function loadPinnedRoots(): PinnedRoot[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(PINNED_ROOTS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as PinnedRoot[];
    return Array.isArray(parsed) ? parsed.slice(0, MAX_PINNED_ROOTS) : [];
  } catch {
    return [];
  }
}

function loadSidebarMode(): LibrarySidebarMode {
  if (typeof localStorage === "undefined") return "vault";
  const stored = localStorage.getItem(SIDEBAR_MODE_KEY);
  if (stored === "files") return "files";
  // Legacy Library tab id — Presentations was renamed to Artifacts.
  if (stored === "presentations" || stored === "artifacts") return "artifacts";
  return "vault";
}

function savePinnedRoots(roots: PinnedRoot[]) {
  localStorage.setItem(PINNED_ROOTS_KEY, JSON.stringify(roots.slice(0, MAX_PINNED_ROOTS)));
}

export class ExternalDeskStore {
  sidebarMode = $state<LibrarySidebarMode>(loadSidebarMode());
  pinnedRoots = $state<PinnedRoot[]>(loadPinnedRoots());
  directories = $state<Record<string, DirectoryState>>({});
  expandedDirectories = $state<Record<string, boolean>>({});
  showHidden = $state(false);
  loadingRoot = $derived(Object.entries(this.directories).find(([, state]) => state.loading)?.[0] ?? null);
  selectedExternalPath = $state<string | null>(null);
  error = $state<string | null>(null);
  searchQuery = $state("");
  /** Session UI — survives Workspace rail remounts. */
  expandedPins = $state<Record<string, boolean>>({});
  private refreshRequests = new Map<string, Promise<void>>();

  searchHitsList = $derived.by((): ExternalFileEntry[] => {
    const query = this.searchQuery.trim().toLowerCase();
    if (!query) return [];
    return this.allEntries
      .filter((entry) => !entry.is_dir)
      .filter((entry) => {
        const haystack = `${entry.name} ${entry.path} ${entry.ext ?? ""}`.toLowerCase();
        return haystack.includes(query);
      });
  });

  get allEntries(): ExternalFileEntry[] {
    return [...new Map(Object.values(this.directories).flatMap((directory) => directory.entries).map((entry) => [entry.path, entry])).values()];
  }

  searchHits(): ExternalFileEntry[] {
    return this.searchHitsList;
  }

  setSidebarMode(mode: LibrarySidebarMode) {
    const previous = this.sidebarMode;
    this.sidebarMode = mode;
    localStorage.setItem(SIDEBAR_MODE_KEY, mode);
    if (mode === "artifacts") {
      void import("$lib/stores/artifacts.svelte").then(({ artifacts }) => artifacts.refresh());
    }
    if (previous === "files" && mode !== "files") {
      void import("$lib/stores/vault.svelte").then(({ vault }) => {
        if (vault.previewPresentation === "pane") {
          vault.closeAttachmentPreview();
        }
      });
    }
  }

  setSearchQuery(query: string) {
    this.searchQuery = query;
  }

  async pinFolder() {
    this.error = null;
    if (!isCoLocatedWorkshop()) {
      this.error = vaultPinFolderRemoteHint();
      return false;
    }
    const path = await pickExternalFolder();
    if (!path) return false;
    if (this.pinnedRoots.some((root) => root.path === path)) {
      this.error = "That folder is already pinned.";
      return false;
    }
    if (this.pinnedRoots.length >= MAX_PINNED_ROOTS) {
      this.error = `You can pin up to ${MAX_PINNED_ROOTS} folders.`;
      return false;
    }
    const root: PinnedRoot = {
      id: randomUuid(),
      path,
      label: rootLabelFromPath(path),
    };
    this.pinnedRoots = [...this.pinnedRoots, root];
    savePinnedRoots(this.pinnedRoots);
    this.expandedPins = { ...this.expandedPins, [root.id]: true };
    this.setSidebarMode("files");
    await this.loadDirectory(path);
    return true;
  }

  unpinRoot(rootId: string) {
    const root = this.pinnedRoots.find((row) => row.id === rootId);
    this.pinnedRoots = this.pinnedRoots.filter((row) => row.id !== rootId);
    savePinnedRoots(this.pinnedRoots);
    if (root) this.releaseDirectoryTree(root.path);
  }

  selectExternalPath(path: string | null) {
    this.selectedExternalPath = path;
  }

  isPinExpanded(rootId: string): boolean {
    return this.expandedPins[rootId] ?? false;
  }

  togglePinExpanded(rootId: string) {
    this.expandedPins = {
      ...this.expandedPins,
      [rootId]: !this.isPinExpanded(rootId),
    };
    const root = this.pinnedRoots.find((root) => root.id === rootId);
    if (root) {
      if (this.isPinExpanded(rootId)) void this.loadDirectory(root.path);
      else this.releaseDirectoryTree(root.path);
    }
  }

  toggleDirectory(path: string) {
    const expanded = !this.expandedDirectories[path];
    this.expandedDirectories = { ...this.expandedDirectories, [path]: expanded };
    if (expanded) void this.loadDirectory(path);
    else this.releaseDirectoryTree(path);
  }

  private releaseDirectoryTree(path: string) {
    this.directories = Object.fromEntries(Object.entries(this.directories).filter(([key]) => !inside(key, path)));
    for (const key of this.refreshRequests.keys()) if (inside(key, path)) this.refreshRequests.delete(key);
  }

  setShowHidden(value: boolean) {
    this.showHidden = value;
    this.directories = {};
    this.refreshRequests.clear();
  }

  async loadDirectory(path: string, refresh = false) {
    if (!refresh && this.directories[path] && !this.directories[path].error) return this.refreshRequests.get(path);
    return this.requestDirectory(path, null, []);
  }

  async changeDirectoryPage(path: string, direction: "next" | "previous") {
    const state = this.directories[path];
    if (!state || state.loading) return;
    if (direction === "next" && state.next_cursor) {
      return this.requestDirectory(path, state.next_cursor, [...state.previous, state.cursor]);
    }
    if (direction === "previous" && state.previous.length) {
      return this.requestDirectory(path, state.previous.at(-1)!, state.previous.slice(0, -1));
    }
  }

  private async requestDirectory(path: string, cursor: string | null, previous: (string | null)[]) {
    if (!isCoLocatedWorkshop() || !this.pinnedRoots.some((root) => inside(path, root.path))) return;
    const pending = this.refreshRequests.get(path);
    if (pending) return pending;
    const prior = this.directories[path] ?? { entries: [], next_cursor: null, cursor: null, previous: [] };
    this.directories = { ...this.directories, [path]: { ...prior, loading: true, error: null } };
    const current = () => this.refreshRequests.get(path) === request;
    const request = listExternalDirectory(path, cursor, this.showHidden).then((page) => {
      if (!current()) return;
      if (!isCoLocatedWorkshop()) { this.releaseDirectoryTree(path); return; }
      if (this.directories[path]?.cursor !== cursor) {
        for (const child of Object.keys(this.directories)) {
          if (child !== path && inside(child, path)) this.releaseDirectoryTree(child);
        }
      }
      this.directories = { ...this.directories, [path]: { ...page, cursor, previous, loading: false, error: null } };
    }).catch((err) => {
      if (current()) this.directories = { ...this.directories, [path]: { ...this.directories[path], loading: false, error: err instanceof Error ? err.message : String(err) } };
    }).finally(() => {
      if (current()) this.refreshRequests.delete(path);
    });
    this.refreshRequests.set(path, request);
    return request;
  }

  async refreshRoot(path: string) { return this.loadDirectory(path, true); }

  async refreshAllRoots() {
    const paths = new Set([...Object.keys(this.directories), ...this.pinnedRoots.filter((root) => this.isPinExpanded(root.id)).map((root) => root.path)]);
    await Promise.all([...paths].map((path) => this.refreshRoot(path)));
  }

  attachmentForPath(path: string) {
    const name = path.split(/[/\\]/).pop() ?? path;
    return {
      path,
      label: name,
      mime: guessMimeFromPath(path),
    };
  }
}

export const externalDesk = new ExternalDeskStore();
