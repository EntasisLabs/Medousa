/**
 * Code workbench location history — one stack per undertaking, group-aware.
 * Composes with shellTabs at the call site (no store import cycle).
 */

export type CodeHistoryEntry = {
  workId: string;
  path: string;
  line: number | null;
  /** Shell editor group that held this location when recorded. */
  groupId: string | null;
};

const HISTORY_CAP = 100;

class CodeWorkbenchState {
  private entriesByWorkId = $state<Record<string, CodeHistoryEntry[]>>({});
  private indexByWorkId = $state<Record<string, number>>({});

  /** Test / workshop-switch reset. */
  reset() {
    this.entriesByWorkId = {};
    this.indexByWorkId = {};
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
