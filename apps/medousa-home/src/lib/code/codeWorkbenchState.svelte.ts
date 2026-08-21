/**
 * Code workbench posture: group-aware history + restorable contextual regions.
 * Composes with shellTabs at the call site (no store import cycle).
 */

export type CodeHistoryEntry = {
  workId: string;
  path: string;
  line: number | null;
  /** Shell editor group that held this location when recorded. */
  groupId: string | null;
};

/** Mutually exclusive side/context panel inside the Code editor chrome. */
export type CodeContextPanel =
  | "problems"
  | "outline"
  | "references"
  | "language"
  | null;

/**
 * Project-scoped Code layout. Shell owns pane geometry and group tab strips;
 * this store owns which optional Code regions are open for a work item.
 */
export type CodeWorkbenchLayout = {
  context_panel: CodeContextPanel;
  terminal: boolean;
  tests: boolean;
  search: boolean;
  changes: boolean;
  /** User-selected project command. Stable task id from the daemon catalog. */
  primary_task: string | null;
};

export const DEFAULT_CODE_WORKBENCH_LAYOUT: CodeWorkbenchLayout = {
  context_panel: null,
  terminal: false,
  tests: false,
  search: false,
  changes: false,
  primary_task: null,
};

const HISTORY_CAP = 100;

const CONTEXT_PANELS = new Set<string>([
  "problems",
  "outline",
  "references",
  "language",
]);

export function normalizeCodeWorkbenchLayout(
  value: unknown,
): CodeWorkbenchLayout {
  if (!value || typeof value !== "object") {
    return { ...DEFAULT_CODE_WORKBENCH_LAYOUT };
  }
  const raw = value as Record<string, unknown>;
  const panel = raw.context_panel;
  return {
    context_panel:
      panel === null || (typeof panel === "string" && CONTEXT_PANELS.has(panel))
        ? (panel as CodeContextPanel)
        : null,
    terminal: raw.terminal === true,
    tests: raw.tests === true,
    search: raw.search === true,
    changes: raw.changes === true,
    primary_task:
      typeof raw.primary_task === "string" && raw.primary_task.trim()
        ? raw.primary_task.trim().slice(0, 160)
        : null,
  };
}

/** Shell + LME composition: Code file tabs visible in one editor group. */
export type VisibleCodeTab = {
  shellTabId: string;
  lmeTabId: string;
  path: string;
};

export function visibleCodeTabsInGroup(input: {
  workId: string;
  groupTabs: Array<{ id: string; kind: string; lmeTabId?: string }>;
  lmeTabs: Array<{
    tabId: string;
    kind: string;
    workId?: string;
    resource?: { kind?: string; path?: string };
  }>;
}): VisibleCodeTab[] {
  const workId = input.workId.trim();
  if (!workId) return [];
  const byLmeId = new Map(
    input.lmeTabs
      .filter(
        (tab) =>
          tab.kind === "code" &&
          tab.workId === workId &&
          tab.resource?.kind === "file" &&
          typeof tab.resource.path === "string",
      )
      .map((tab) => [tab.tabId, tab.resource!.path!]),
  );
  const out: VisibleCodeTab[] = [];
  for (const shell of input.groupTabs) {
    if (shell.kind !== "lme" || !shell.lmeTabId) continue;
    const path = byLmeId.get(shell.lmeTabId);
    if (!path) continue;
    out.push({
      shellTabId: shell.id,
      lmeTabId: shell.lmeTabId,
      path,
    });
  }
  return out;
}

class CodeWorkbenchState {
  private entriesByWorkId = $state<Record<string, CodeHistoryEntry[]>>({});
  private indexByWorkId = $state<Record<string, number>>({});
  private layoutByWorkId = $state<Record<string, CodeWorkbenchLayout>>({});

  /** Test / workshop-switch reset. */
  reset() {
    this.entriesByWorkId = {};
    this.indexByWorkId = {};
    this.layoutByWorkId = {};
  }

  /** Snapshot for tests and adapters. */
  entriesFor(workId: string): CodeHistoryEntry[] {
    return this.entriesByWorkId[workId] ?? [];
  }

  indexFor(workId: string): number {
    const entries = this.entriesFor(workId);
    return this.indexByWorkId[workId] ?? entries.length - 1;
  }

  current(workId: string): CodeHistoryEntry | null {
    const entries = this.entriesFor(workId);
    const index = this.indexFor(workId);
    return entries[index] ?? null;
  }

  layoutFor(workId: string): CodeWorkbenchLayout {
    return this.layoutByWorkId[workId] ?? { ...DEFAULT_CODE_WORKBENCH_LAYOUT };
  }

  applyLayout(workId: string, layout: unknown) {
    if (!workId) return;
    this.layoutByWorkId = {
      ...this.layoutByWorkId,
      [workId]: normalizeCodeWorkbenchLayout(layout),
    };
  }

  patchLayout(workId: string, patch: Partial<CodeWorkbenchLayout>) {
    if (!workId) return;
    const next = {
      ...this.layoutFor(workId),
      ...patch,
    };
    this.layoutByWorkId = { ...this.layoutByWorkId, [workId]: next };
  }

  setContextPanel(workId: string, panel: CodeContextPanel) {
    this.patchLayout(workId, { context_panel: panel });
  }

  setTerminalOpen(workId: string, open: boolean) {
    this.patchLayout(workId, { terminal: open });
  }

  setTestsOpen(workId: string, open: boolean) {
    this.patchLayout(workId, { tests: open });
  }

  setSearchOpen(workId: string, open: boolean) {
    this.patchLayout(workId, { search: open });
  }

  setChangesOpen(workId: string, open: boolean) {
    this.patchLayout(workId, { changes: open });
  }

  setPrimaryTask(workId: string, taskId: string | null) {
    this.patchLayout(workId, {
      primary_task: taskId?.trim().slice(0, 160) || null,
    });
  }

  record(
    workId: string,
    path: string,
    line: number | null,
    groupId: string | null = null,
  ) {
    if (!workId || !path) return;
    const next: CodeHistoryEntry = {
      workId,
      path,
      line: line && line > 0 ? Math.floor(line) : null,
      groupId,
    };
    const history = this.entriesFor(workId);
    const index = this.indexFor(workId);
    const current = history[index];
    if (
      current?.path === next.path &&
      current.line === next.line &&
      current.groupId === next.groupId
    ) {
      return;
    }
    const entries = [...history.slice(0, index + 1), next].slice(-HISTORY_CAP);
    this.entriesByWorkId = { ...this.entriesByWorkId, [workId]: entries };
    this.indexByWorkId = { ...this.indexByWorkId, [workId]: entries.length - 1 };
  }

  canNavigate(workId: string, direction: -1 | 1): boolean {
    const entries = this.entriesFor(workId);
    const index = this.indexFor(workId);
    return direction < 0 ? index > 0 : index >= 0 && index < entries.length - 1;
  }

  /**
   * Step the history cursor. Caller focuses `groupId` via shellTabs when set.
   */
  step(workId: string, direction: -1 | 1): CodeHistoryEntry | null {
    if (!this.canNavigate(workId, direction)) return null;
    const entries = this.entriesFor(workId);
    const current = this.indexFor(workId);
    const index = current + direction;
    const location = entries[index];
    if (!location) return null;
    this.indexByWorkId = { ...this.indexByWorkId, [workId]: index };
    return location;
  }

  /** Undo a failed open after step(). */
  restoreIndex(workId: string, index: number) {
    this.indexByWorkId = { ...this.indexByWorkId, [workId]: index };
  }
}

export const codeWorkbenchState = new CodeWorkbenchState();
