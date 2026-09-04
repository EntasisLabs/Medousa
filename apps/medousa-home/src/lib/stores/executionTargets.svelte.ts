import {
  getExecutionTargets,
  type ExecutionTargetInventory,
  type ExecutionTargetInventoryEntry,
  type ExecutionTargetSelection,
} from "$lib/daemon/runtime";
import {
  activeWorkshopId,
  workshopScopedStorageKey,
} from "$lib/utils/workshopLocality";

const STORAGE_PREFIX = "medousa-home-worker-targets-v1";
const MAX_REMEMBERED_SESSIONS = 100;

type SelectionMap = Record<string, ExecutionTargetSelection>;

function normalizedSelection(value: unknown): ExecutionTargetSelection | null {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Record<string, unknown>;
  if (candidate.kind === "same_as_parent") return { kind: "same_as_parent" };
  if (candidate.kind === "auto") return { kind: "auto" };
  if (candidate.kind !== "exact" || typeof candidate.runtime_id !== "string") return null;
  const runtimeId = candidate.runtime_id.trim();
  return runtimeId ? { kind: "exact", runtime_id: runtimeId } : null;
}

function loadSelections(workshopId: string): SelectionMap {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(workshopScopedStorageKey(STORAGE_PREFIX, workshopId));
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const selections: SelectionMap = {};
    for (const [sessionId, value] of Object.entries(parsed)) {
      const id = sessionId.trim();
      const selection = normalizedSelection(value);
      if (id && selection) selections[id] = selection;
    }
    return selections;
  } catch {
    return {};
  }
}

function compactRuntimeId(runtimeId: string): string {
  const id = runtimeId.trim();
  if (id.length <= 24) return id;
  return `${id.slice(0, 10)}…${id.slice(-6)}`;
}

export class ExecutionTargetStore {
  inventory = $state<ExecutionTargetInventory | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  workshopScopeId = $state("");
  selections = $state<SelectionMap>({});

  private epoch = 0;
  private loaded = false;
  private refreshInFlight: Promise<void> | null = null;

  constructor(
    private readonly loadInventory: () => Promise<ExecutionTargetInventory> =
      getExecutionTargets,
  ) {
    this.activateWorkshopScope(activeWorkshopId());
  }

  resetForWorkshopSwitch() {
    this.epoch += 1;
    this.inventory = null;
    this.loading = false;
    this.error = null;
    this.loaded = false;
    this.refreshInFlight = null;
    this.workshopScopeId = "";
    this.selections = {};
  }

  activateWorkshopScope(workshopId: string) {
    const scope = workshopId.trim() || "personal";
    if (scope === this.workshopScopeId) return;
    this.resetForWorkshopSwitch();
    this.workshopScopeId = scope;
    this.selections = loadSelections(scope);
  }

  async refresh(options: { force?: boolean } = {}): Promise<void> {
    if (this.loaded && !options.force) return;
    if (this.refreshInFlight) return this.refreshInFlight;
    const epoch = this.epoch;
    this.loading = true;
    this.error = null;
    const request = this.loadInventory()
      .then((inventory) => {
        if (epoch !== this.epoch) return;
        this.inventory = inventory;
        this.loaded = true;
      })
      .catch((error) => {
        if (epoch !== this.epoch) return;
        this.error = error instanceof Error ? error.message : String(error);
        throw error;
      })
      .finally(() => {
        if (epoch !== this.epoch) return;
        this.loading = false;
        this.refreshInFlight = null;
      });
    this.refreshInFlight = request;
    return request;
  }

  selectionFor(sessionId: string): ExecutionTargetSelection | null {
    return this.selections[sessionId.trim()] ?? null;
  }

  setSelection(sessionId: string, selection: ExecutionTargetSelection | null) {
    const id = sessionId.trim();
    if (!id) return;
    const next = { ...this.selections };
    if (selection) next[id] = selection;
    else delete next[id];
    this.selections = next;
    this.persistSelections();
  }

  turnSelection(sessionId: string): ExecutionTargetSelection | null {
    const id = sessionId.trim();
    const selection = this.selectionFor(id);
    if (!selection || selection.kind !== "auto") return selection;
    return {
      kind: "auto",
      requirements: {
        ...(selection.requirements ?? {}),
        selection_key: `session:${id}`,
      },
    };
  }

  userTargets(): ExecutionTargetInventoryEntry[] {
    return (this.inventory?.targets ?? []).filter((target) => target.user_selectable);
  }

  agentTargets(): ExecutionTargetInventoryEntry[] {
    return (this.inventory?.targets ?? []).filter((target) => target.agent_selectable);
  }

  defaultRuntimeId(): string | null {
    return (
      this.inventory?.default_runtime_id?.trim() ||
      this.inventory?.parent_runtime_id?.trim() ||
      null
    );
  }

  parentTarget(): ExecutionTargetInventoryEntry | null {
    const parentId = this.inventory?.parent_runtime_id?.trim();
    if (!parentId) return null;
    return this.userTargets().find((target) => target.runtime_id === parentId) ?? null;
  }

  /** Tauri needs an override only when execution leaves the parent daemon. */
  transportRuntimeId(runtimeId?: string | null): string | null {
    const id = runtimeId?.trim();
    if (!id || id === this.inventory?.parent_runtime_id?.trim()) return null;
    return id;
  }

  runtimeLabel(runtimeId?: string | null): string | null {
    const id = runtimeId?.trim();
    if (!id || id === "unknown") return null;
    const target = this.inventory?.targets.find((candidate) => candidate.runtime_id === id);
    if (target?.label.trim()) return target.label.trim();
    if (id === this.inventory?.parent_runtime_id?.trim()) return "This workshop";
    return compactRuntimeId(id);
  }

  selectionLabel(sessionId: string): string {
    const selection = this.selectionFor(sessionId);
    if (!selection) return this.runtimeLabel(this.defaultRuntimeId()) ?? "Default";
    if (selection.kind === "same_as_parent") {
      return this.runtimeLabel(this.inventory?.parent_runtime_id) ?? "This workshop";
    }
    if (selection.kind === "exact") {
      return this.runtimeLabel(selection.runtime_id) ?? "Unavailable workshop";
    }
    return "Auto";
  }

  selectionUnavailable(sessionId: string): boolean {
    const selection = this.selectionFor(sessionId);
    if (!selection || !this.inventory) return false;
    if (selection.kind === "same_as_parent") return !this.parentTarget();
    if (selection.kind === "auto") return this.agentTargets().length === 0;
    return !this.userTargets().some((target) => target.runtime_id === selection.runtime_id);
  }

  shouldShow(sessionId: string): boolean {
    if (this.selectionFor(sessionId)) return true;
    const targets = this.userTargets();
    if (targets.length > 1) return true;
    const defaultId = this.defaultRuntimeId();
    const parentId = this.inventory?.parent_runtime_id?.trim();
    return Boolean(defaultId && parentId && defaultId !== parentId);
  }

  private persistSelections() {
    if (!this.workshopScopeId || typeof localStorage === "undefined") return;
    const entries = Object.entries(this.selections).slice(-MAX_REMEMBERED_SESSIONS);
    localStorage.setItem(
      workshopScopedStorageKey(STORAGE_PREFIX, this.workshopScopeId),
      JSON.stringify(Object.fromEntries(entries)),
    );
  }
}

export const executionTargets = new ExecutionTargetStore();
