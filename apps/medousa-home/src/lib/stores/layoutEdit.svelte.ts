import type { LayoutNode } from "$lib/types/environment";
import { environment } from "$lib/stores/environment.svelte";
import { applyLayoutRoot, ensureSurfaceLayoutRoot } from "$lib/utils/environmentCanvasOps";
import {
  addSlotNode,
  assignComponentToSlot,
  cloneLayoutNode,
  moveComponentToSlot,
  splitSelectionHorizontal,
  splitSelectionVertical,
  validateLayoutClient,
} from "$lib/utils/layoutEditOps";
import { resolveLayoutRoot } from "$lib/utils/layoutPresentation";

class LayoutEditStore {
  editingSurfaceId = $state<string | null>(null);
  draftRoot = $state<LayoutNode | null>(null);
  baselineRoot = $state<LayoutNode | null>(null);
  selectedId = $state<string | null>(null);
  movingId = $state<string | null>(null);
  error = $state<string | null>(null);
  saving = $state(false);

  isEditingSurface(surfaceId: string): boolean {
    return this.editingSurfaceId === surfaceId;
  }

  activeRoot(surfaceId: string, fallback: LayoutNode): LayoutNode {
    if (this.editingSurfaceId === surfaceId) {
      if (this.draftRoot) return this.draftRoot;
      return fallback;
    }
    return fallback;
  }

  begin(surfaceId: string, focusComponentId?: string | null): void {
    const spec = environment.spec;
    if (!spec || !environment.isCustomSurface(surfaceId)) return;
    const surface = environment.surfaceById(surfaceId);
    if (!surface) return;
    const resolved = resolveLayoutRoot(surface, spec.components);
    this.editingSurfaceId = surfaceId;
    this.baselineRoot = cloneLayoutNode(resolved);
    this.draftRoot = cloneLayoutNode(resolved);
    this.selectedId = focusComponentId ?? null;
    this.movingId = null;
    this.error = null;
  }

  cancel(): void {
    this.editingSurfaceId = null;
    this.draftRoot = null;
    this.baselineRoot = null;
    this.selectedId = null;
    this.movingId = null;
    this.error = null;
  }

  select(id: string | null): void {
    this.selectedId = id;
  }

  pickUp(componentId: string): void {
    this.movingId = componentId;
    this.selectedId = componentId;
  }

  cancelMove(): void {
    this.movingId = null;
  }

  dropOnSlot(slotId: string): void {
    if (!this.draftRoot || !this.movingId) return;
    this.draftRoot = moveComponentToSlot(this.draftRoot, this.movingId, slotId);
    this.movingId = null;
    this.selectedId = slotId;
  }

  dropOnComponent(targetComponentId: string): void {
    if (!this.draftRoot || !this.movingId || this.movingId === targetComponentId) return;
    const slotId = `zone-drop-${Date.now()}`;
    let root = addSlotNode(this.draftRoot, slotId);
    root = moveComponentToSlot(root, this.movingId, slotId);
    this.draftRoot = root;
    this.movingId = null;
    this.selectedId = targetComponentId;
  }

  addZone(): void {
    if (!this.draftRoot) return;
    this.draftRoot = addSlotNode(this.draftRoot);
  }

  splitHorizontal(): void {
    if (!this.draftRoot || !this.selectedId) return;
    if (this.selectedId.startsWith("zone-")) return;
    this.draftRoot = splitSelectionHorizontal(this.draftRoot, this.selectedId);
  }

  splitVertical(): void {
    if (!this.draftRoot || !this.selectedId) return;
    if (this.selectedId.startsWith("zone-")) return;
    this.draftRoot = splitSelectionVertical(this.draftRoot, this.selectedId);
  }

  resetLayout(): void {
    this.draftRoot = null;
    this.selectedId = null;
    this.movingId = null;
  }

  async save(): Promise<boolean> {
    const surfaceId = this.editingSurfaceId;
    const spec = environment.spec;
    if (!surfaceId || !spec) return false;
    const surface = environment.surfaceById(surfaceId);
    if (!surface) return false;

    const fallback = resolveLayoutRoot(surface, spec.components);
    const nextRoot = this.draftRoot ? cloneLayoutNode(this.draftRoot) : null;
    if (nextRoot) {
      const errors = validateLayoutClient(nextRoot);
      if (errors.length > 0) {
        this.error = errors[0] ?? "Invalid layout";
        return false;
      }
    }

    this.saving = true;
    this.error = null;
    try {
      const updated = applyLayoutRoot(spec, surfaceId, nextRoot);
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

export function layoutRootForEditing(surfaceId: string): LayoutNode | null {
  const spec = environment.spec;
  const surface = environment.surfaceById(surfaceId);
  if (!spec || !surface) return null;
  const resolved = resolveLayoutRoot(surface, spec.components);
  return layoutEdit.activeRoot(surfaceId, resolved);
}

export function ensureDraftForSurface(surfaceId: string): LayoutNode | null {
  const spec = environment.spec;
  const surface = environment.surfaceById(surfaceId);
  if (!spec || !surface) return null;
  const resolved = resolveLayoutRoot(surface, spec.components);
  return ensureSurfaceLayoutRoot(spec, surfaceId, resolved);
}
