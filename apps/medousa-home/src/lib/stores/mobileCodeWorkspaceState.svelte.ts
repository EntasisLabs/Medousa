import {
  resolveMobileCodeFilesFilter,
  type MobileCodeChromeMode,
  type MobileCodeFilesFilter,
  type MobileCodeJumpOrigin,
  type MobileCodeSurface,
} from "$lib/utils/mobileCodeLanding";

export type {
  MobileCodeChromeMode,
  MobileCodeFilesFilter,
  MobileCodeJumpOrigin,
  MobileCodeSurface,
};

export type MobileCodePresentation = {
  surface: MobileCodeSurface;
  /** Previous sibling room after a switcher tap. Not a jump. */
  lastRoom: MobileCodeSurface | null;
  /** Origin of a file/hunk/`gf` jump into Editor. */
  jumpOrigin: MobileCodeJumpOrigin | null;
  filesFilter: MobileCodeFilesFilter | "auto";
  filesDirectory: string;
  lastOpenedPath: string | null;
  changesPath: string | null;
  terminalSessionId: string | null;
  ctrlLatch: boolean;
};

function emptyPresentation(surface: MobileCodeSurface): MobileCodePresentation {
  return {
    surface,
    lastRoom: null,
    jumpOrigin: null,
    filesFilter: "auto",
    filesDirectory: "",
    lastOpenedPath: null,
    changesPath: null,
    terminalSessionId: null,
    ctrlLatch: false,
  };
}

class MobileCodeWorkspaceStore {
  selectedWorkId = $state<string | null>(null);
  byWorkId = $state<Record<string, MobileCodePresentation>>({});
  filesQuery = $state("");
  ancestorSheetOpen = $state(false);
  fileSwitcherOpen = $state(false);
  overflowOpen = $state(false);
  sessionSheetOpen = $state(false);

  readonly presentation = $derived.by((): MobileCodePresentation | null => {
    const workId = this.selectedWorkId;
    if (!workId) return null;
    return this.byWorkId[workId] ?? null;
  });

  readonly surface = $derived(
    this.presentation?.surface ?? null,
  );

  readonly chromeMode = $derived.by((): MobileCodeChromeMode => {
    if (!this.selectedWorkId) return "projects";
    return this.presentation?.surface ?? "files";
  });

  readonly inProject = $derived(Boolean(this.selectedWorkId));

  resetForWorkshopSwitch() {
    this.selectedWorkId = null;
    this.byWorkId = {};
    this.filesQuery = "";
    this.closeDetails();
  }

  closeDetails() {
    this.ancestorSheetOpen = false;
    this.fileSwitcherOpen = false;
    this.overflowOpen = false;
    this.sessionSheetOpen = false;
  }

  enterProject(workId: string, landing: MobileCodeSurface) {
    const id = workId.trim();
    if (!id) return;
    this.closeDetails();
    this.filesQuery = "";
    const existing = this.byWorkId[id];
    this.selectedWorkId = id;
    if (existing) {
      if (landing === "changes" && existing.surface !== "changes") {
        this.byWorkId = {
          ...this.byWorkId,
          [id]: {
            ...existing,
            lastRoom: existing.surface,
            surface: "changes",
            jumpOrigin: null,
          },
        };
      }
      return;
    }
    this.byWorkId = {
      ...this.byWorkId,
      [id]: emptyPresentation(landing),
    };
  }

  leaveProject() {
    this.closeDetails();
    this.filesQuery = "";
    this.selectedWorkId = null;
  }

  switchRoom(surface: MobileCodeSurface) {
    const workId = this.selectedWorkId;
    const current = workId ? this.byWorkId[workId] : null;
    if (!workId || !current || current.surface === surface) return;
    this.closeDetails();
    this.byWorkId = {
      ...this.byWorkId,
      [workId]: {
        ...current,
        lastRoom: current.surface,
        surface,
      },
    };
  }

  jumpToEditor(origin: MobileCodeJumpOrigin, path?: string | null) {
    const workId = this.selectedWorkId;
    const current = workId ? this.byWorkId[workId] : null;
    if (!workId || !current) return;
    const opened = path?.trim() || current.lastOpenedPath;
    if (current.surface === "editor") {
      this.patch({ lastOpenedPath: opened });
      return;
    }
    this.closeDetails();
    this.byWorkId = {
      ...this.byWorkId,
      [workId]: {
        ...current,
        lastRoom: current.surface,
        surface: "editor",
        jumpOrigin: origin,
        lastOpenedPath: opened,
      },
    };
  }

  setFilesFilter(filter: MobileCodeFilesFilter | "auto") {
    this.patch({ filesFilter: filter });
  }

  setFilesDirectory(directory: string) {
    this.patch({ filesDirectory: directory.replace(/^\/+|\/+$/g, "") });
  }

  setLastOpenedPath(path: string | null) {
    this.patch({ lastOpenedPath: path });
  }

  setChangesPath(path: string | null) {
    this.patch({ changesPath: path });
  }

  setTerminalSessionId(sessionId: string | null) {
    this.patch({ terminalSessionId: sessionId });
  }

  setCtrlLatch(latched: boolean) {
    this.patch({ ctrlLatch: latched });
  }

  toggleCtrlLatch() {
    const current = this.presentation;
    this.patch({ ctrlLatch: !current?.ctrlLatch });
  }

  resolvedFilesFilter(input: {
    hasChangedFiles: boolean;
    hasRecentFiles: boolean;
  }): MobileCodeFilesFilter {
    const current = this.presentation;
    if (!current || current.filesFilter === "auto") {
      return resolveMobileCodeFilesFilter(input);
    }
    return current.filesFilter;
  }

  /**
   * Hardware / chrome back after sheets. Returns true when consumed.
   * Jump pops first, then the previous sibling room, then the project list.
   */
  handleBack(): boolean {
    if (this.ancestorSheetOpen || this.fileSwitcherOpen || this.overflowOpen || this.sessionSheetOpen) {
      this.closeDetails();
      return true;
    }
    if (this.filesQuery.trim()) {
      this.filesQuery = "";
      return true;
    }
    const workId = this.selectedWorkId;
    const current = workId ? this.byWorkId[workId] : null;
    if (!workId || !current) return false;

    if (current.surface === "editor" && current.jumpOrigin) {
      const origin = current.jumpOrigin;
      this.byWorkId = {
        ...this.byWorkId,
        [workId]: {
          ...current,
          surface: origin,
          jumpOrigin: null,
          lastRoom: null,
        },
      };
      return true;
    }

    if (current.lastRoom && current.lastRoom !== current.surface) {
      const target = current.lastRoom;
      this.byWorkId = {
        ...this.byWorkId,
        [workId]: {
          ...current,
          surface: target,
          lastRoom: null,
        },
      };
      return true;
    }

    this.leaveProject();
    return true;
  }

  private patch(partial: Partial<MobileCodePresentation>) {
    const workId = this.selectedWorkId;
    const current = workId ? this.byWorkId[workId] : null;
    if (!workId || !current) return;
    this.byWorkId = {
      ...this.byWorkId,
      [workId]: { ...current, ...partial },
    };
  }
}

export const mobileCodeWorkspaceState = new MobileCodeWorkspaceStore();
