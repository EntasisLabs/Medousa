import type { ExternalFileEntry, LibrarySidebarMode, PinnedRoot } from "$lib/types/externalDesk";
import {
  pickExternalFolder,
  rootLabelFromPath,
  scanExternalRoot,
} from "$lib/utils/externalDeskApi";
import { guessMimeFromPath } from "$lib/utils/vaultAttachments";
import {
  isCoLocatedWorkshop,
  vaultPinFolderRemoteHint,
} from "$lib/utils/workshopLocality";

const PINNED_ROOTS_KEY = "medousa-home-pinned-roots";
const SIDEBAR_MODE_KEY = "medousa-home-library-sidebar-mode";
const MAX_PINNED_ROOTS = 5;

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
  if (stored === "presentations") return "presentations";
  return "vault";
}

function savePinnedRoots(roots: PinnedRoot[]) {
  localStorage.setItem(PINNED_ROOTS_KEY, JSON.stringify(roots.slice(0, MAX_PINNED_ROOTS)));
}

export class ExternalDeskStore {
  sidebarMode = $state<LibrarySidebarMode>(loadSidebarMode());
  pinnedRoots = $state<PinnedRoot[]>(loadPinnedRoots());
  entriesByRoot = $state<Record<string, ExternalFileEntry[]>>({});
  loadingRoot = $state<string | null>(null);
  selectedExternalPath = $state<string | null>(null);
  error = $state<string | null>(null);
  searchQuery = $state("");
  /** Session UI — survives Workspace rail remounts. */
  expandedPins = $state<Record<string, boolean>>({});
  showAllByRoot = $state<Record<string, boolean>>({});

  searchHitsList = $derived.by((): ExternalFileEntry[] => {
    const query = this.searchQuery.trim().toLowerCase();
    if (!query) return [];
    return Object.values(this.entriesByRoot)
      .flat()
      .filter((entry) => !entry.is_dir)
      .filter((entry) => {
        const haystack = `${entry.name} ${entry.path} ${entry.ext ?? ""}`.toLowerCase();
        return haystack.includes(query);
      })
      .slice(0, 20);
  });

  get allEntries(): ExternalFileEntry[] {
    return Object.values(this.entriesByRoot).flat();
  }

  searchHits(): ExternalFileEntry[] {
    return this.searchHitsList;
  }

  setSidebarMode(mode: LibrarySidebarMode) {
    const previous = this.sidebarMode;
    this.sidebarMode = mode;
    localStorage.setItem(SIDEBAR_MODE_KEY, mode);
    if (mode === "files" && this.pinnedRoots.length > 0) {
      void this.refreshAllRoots();
    }
    if (mode === "presentations") {
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
      id: crypto.randomUUID(),
      path,
      label: rootLabelFromPath(path),
    };
    this.pinnedRoots = [...this.pinnedRoots, root];
    savePinnedRoots(this.pinnedRoots);
    await this.refreshRoot(path);
    this.setSidebarMode("files");
    return true;
  }

  unpinRoot(rootId: string) {
    const root = this.pinnedRoots.find((row) => row.id === rootId);
    this.pinnedRoots = this.pinnedRoots.filter((row) => row.id !== rootId);
    savePinnedRoots(this.pinnedRoots);
    if (root) {
      const next = { ...this.entriesByRoot };
      delete next[root.path];
      this.entriesByRoot = next;
    }
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
  }

  isShowAll(rootId: string): boolean {
    return this.showAllByRoot[rootId] ?? false;
  }

  setShowAll(rootId: string, value: boolean) {
    this.showAllByRoot = { ...this.showAllByRoot, [rootId]: value };
  }

  async refreshRoot(rootPath: string) {
    this.loadingRoot = rootPath;
    this.error = null;
    try {
      const entries = await scanExternalRoot(rootPath);
      this.entriesByRoot = { ...this.entriesByRoot, [rootPath]: entries };
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      if (this.loadingRoot === rootPath) {
        this.loadingRoot = null;
      }
    }
  }

  async refreshAllRoots() {
    await Promise.all(this.pinnedRoots.map((root) => this.refreshRoot(root.path)));
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
