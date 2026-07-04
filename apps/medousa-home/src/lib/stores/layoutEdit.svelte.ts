import type { LayoutNode } from "$lib/types/environment";
import { environment } from "$lib/stores/environment.svelte";
import { applyLayoutRoot, removeComponentFromSpec } from "$lib/utils/environmentCanvasOps";
import { validateLayoutClient } from "$lib/utils/layoutEditOps";
import { mainComponentsForSurface, resolveLayoutRoot } from "$lib/utils/layoutPresentation";
import {
  assignComponentToPane,
  canMergePane,
  clearPaneComponent,
  cloneTilingNode,
  collectComponentIds,
  ensureComponentsPresent,
  findPane,
  findPaneByComponent,
  firstEmptyPaneId,
  firstPaneId,
  layoutRootToTiling,
  mergePane,
  moveComponentToPane,
  splitPane,
  tilingToLayoutRoot,
  type TilingNode,
} from "$lib/utils/layoutTiling";
import { cancelLayoutPointerDrag } from "$lib/utils/layoutEditPointerDrag";

class LayoutEditStore {
  editingSurfaceId = $state<string | null>(null);
  tilingRoot = $state<TilingNode | null>(null);
  selectedPaneId = $state<string | null>(null);
  draftRoot = $state<LayoutNode | null>(null);
  movingId = $state<string | null>(null);
  dragPayloadId = $state<string | null>(null);
  dropTargetId = $state<string | null>(null);
  widgetPickerOpen = $state(false);
  removedDuringEdit = $state<string[]>([]);
  error = $state<string | null>(null);
  saving = $state(false);

  componentCount(surfaceId: string): number {
    return this.componentIds(surfaceId).length;
  }

  isEditingSurface(surfaceId: string): boolean {
    return this.editingSurfaceId === surfaceId;
  }

  canMergeSelected(): boolean {
    if (!this.tilingRoot || !this.selectedPaneId) return false;
    return canMergePane(this.tilingRoot, this.selectedPaneId);
  }

  canRemoveSelected(): boolean {
    if (!this.tilingRoot || !this.selectedPaneId) return false;
    const pane = findPane(this.tilingRoot, this.selectedPaneId);
    return pane?.componentId != null;
  }

  removeSelectedWidget(): void {
    if (!this.tilingRoot || !this.selectedPaneId) return;
    const pane = findPane(this.tilingRoot, this.selectedPaneId);
    if (!pane?.componentId) return;
    this.removeWidget(this.selectedPaneId);
  }

  removeWidget(paneId: string): void {
    if (!this.tilingRoot) return;
    const pane = findPane(this.tilingRoot, paneId);
    if (!pane?.componentId) return;
    this.markRemoved(pane.componentId);
    this.tilingRoot = cloneTilingNode(clearPaneComponent(this.tilingRoot, paneId));
    this.selectedPaneId = paneId;
    this.syncDraftFromTiling();
  }

  private markRemoved(componentId: string): void {
    if (!this.removedDuringEdit.includes(componentId)) {
      this.removedDuringEdit = [...this.removedDuringEdit, componentId];
    }
  }

  private componentIds(surfaceId: string): string[] {
    const spec = environment.spec;
    if (!spec) return [];
    return mainComponentsForSurface(surfaceId, spec.components)
      .map((component) => component.id)
      .filter((id) => !this.removedDuringEdit.includes(id));
  }

  private syncDraftFromTiling(): void {
    if (!this.tilingRoot) {
      this.draftRoot = null;
      return;
    }
    this.draftRoot = tilingToLayoutRoot(this.tilingRoot);
  }

  activeRoot(surfaceId: string, fallback: LayoutNode): LayoutNode {
    if (this.editingSurfaceId === surfaceId && this.draftRoot) {
      return this.draftRoot;
    }
    return fallback;
  }

  begin(surfaceId: string, focusComponentId?: string | null): void {
    const spec = environment.spec;
    if (!spec || !environment.isCustomSurface(surfaceId)) return;
    const surface = environment.surfaceById(surfaceId);
    if (!surface) return;

    const componentIds = this.componentIds(surfaceId);
    const resolved = surface.layoutRoot ? resolveLayoutRoot(surface, spec.components) : null;
    this.tilingRoot = layoutRootToTiling(resolved, componentIds);
    this.editingSurfaceId = surfaceId;
    this.selectedPaneId = focusComponentId
      ? (findPaneByComponent(this.tilingRoot, focusComponentId)?.id ?? firstPaneId(this.tilingRoot))
      : firstPaneId(this.tilingRoot);
    this.syncDraftFromTiling();
    this.movingId = null;
    this.dragPayloadId = null;
    this.dropTargetId = null;
    this.widgetPickerOpen = false;
    this.removedDuringEdit = [];
    this.error = null;
  }

  cancel(): void {
    this.editingSurfaceId = null;
    this.tilingRoot = null;
    this.selectedPaneId = null;
    this.draftRoot = null;
    this.movingId = null;
    this.dragPayloadId = null;
    this.dropTargetId = null;
    this.widgetPickerOpen = false;
    this.removedDuringEdit = [];
    this.error = null;
    cancelLayoutPointerDrag();
  }

  selectPane(paneId: string): void {
    this.selectedPaneId = paneId;
  }

  splitSelected(direction: "horizontal" | "vertical"): void {
    if (!this.tilingRoot || !this.selectedPaneId) return;
    this.tilingRoot = splitPane(this.tilingRoot, this.selectedPaneId, direction);
    this.syncDraftFromTiling();
  }

  mergeSelected(): void {
    if (!this.tilingRoot || !this.selectedPaneId) return;
    this.tilingRoot = mergePane(this.tilingRoot, this.selectedPaneId);
    this.syncDraftFromTiling();
  }

  openWidgetPicker(): void {
    this.widgetPickerOpen = true;
  }

  closeWidgetPicker(): void {
    this.widgetPickerOpen = false;
  }

  onWidgetAdded(surfaceId: string, componentId: string): void {
    if (!this.tilingRoot || this.editingSurfaceId !== surfaceId) return;
    const targetPaneId = this.resolveTargetPaneForNewWidget();
    this.tilingRoot = assignComponentToPane(this.tilingRoot, targetPaneId, componentId);
    this.tilingRoot = ensureComponentsPresent(this.tilingRoot, this.componentIds(surfaceId));
    this.selectedPaneId = targetPaneId;
    this.syncDraftFromTiling();
    this.widgetPickerOpen = false;
  }

  private resolveTargetPaneForNewWidget(): string {
    if (!this.tilingRoot) return "";
    if (this.selectedPaneId) {
      const selected = findPane(this.tilingRoot, this.selectedPaneId);
      if (selected && !selected.componentId) return this.selectedPaneId;
    }
    return firstEmptyPaneId(this.tilingRoot) ?? this.selectedPaneId ?? firstPaneId(this.tilingRoot);
  }

  pickUp(componentId: string): void {
    this.movingId = componentId;
    this.dragPayloadId = componentId;
    this.dropTargetId = null;
  }

  finishDrag(): void {
    this.movingId = null;
    this.dragPayloadId = null;
    this.dropTargetId = null;
  }

  setDropTarget(id: string | null): void {
    if (!this.movingId && !this.dragPayloadId) return;
    this.dropTargetId = id;
  }

  dropOnPane(paneId: string, componentId?: string | null): void {
    const movingId = componentId?.trim() || this.movingId || this.dragPayloadId;
    if (!this.tilingRoot || !movingId) return;
    const next = moveComponentToPane(this.tilingRoot, movingId, paneId);
    this.tilingRoot = cloneTilingNode(next);
    this.selectedPaneId = paneId;
    this.syncDraftFromTiling();
    this.finishDrag();
  }

  async save(): Promise<boolean> {
    const surfaceId = this.editingSurfaceId;
    const spec = environment.spec;
    if (!surfaceId || !spec || !this.tilingRoot) return false;

    const nextRoot = tilingToLayoutRoot(this.tilingRoot);
    const errors = validateLayoutClient(nextRoot);
    if (errors.length > 0) {
      this.error = errors[0] ?? "Invalid layout";
      return false;
    }

    this.saving = true;
    this.error = null;
    try {
      let updated = applyLayoutRoot(spec, surfaceId, nextRoot);
      const keptIds = new Set(collectComponentIds(this.tilingRoot));
      const originalMainIds = mainComponentsForSurface(surfaceId, spec.components).map(
        (component) => component.id,
      );
      for (const componentId of componentIdsToPruneOnSave(
        this.removedDuringEdit,
        originalMainIds,
        keptIds,
      )) {
        removeComponentFromSpec(updated, componentId);
      }
      await environment.saveSpec(updated);
      this.cancel();
      return true;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return false;
    } finally {
      this.saving = false;
    }
  }
}

export const layoutEdit = new LayoutEditStore();

export function componentIdsToPruneOnSave(
  removedDuringEdit: readonly string[],
  originalMainIds: readonly string[],
  keptIds: ReadonlySet<string>,
): string[] {
  const toRemove = new Set<string>(removedDuringEdit);
  for (const componentId of originalMainIds) {
    if (!keptIds.has(componentId)) {
      toRemove.add(componentId);
    }
  }
  return [...toRemove];
}

export function layoutRootForEditing(surfaceId: string): LayoutNode | null {
  const spec = environment.spec;
  const surface = environment.surfaceById(surfaceId);
  if (!spec || !surface) return null;
  const resolved = resolveLayoutRoot(surface, spec.components);
  return layoutEdit.activeRoot(surfaceId, resolved);
}
