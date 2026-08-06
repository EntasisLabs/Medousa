/** Multi-select state for the scripts library rail (LME + workbench). */

import { rangePathsBetween } from "$lib/utils/vaultRailSelection";

export class ScriptLibrarySelectionStore {
  selectedIds = $state<Set<string>>(new Set());
  anchorId = $state<string | null>(null);

  isSelected(scriptId: string): boolean {
    return this.selectedIds.has(scriptId);
  }

  clear() {
    this.selectedIds = new Set();
    this.anchorId = null;
  }

  /**
   * Apply click modifiers for library multi-select.
   * Returns true when the script should open in the editor.
   */
  applySelection(
    scriptId: string,
    event: MouseEvent | null | undefined,
    orderedIds: string[],
  ): boolean {
    if (event?.shiftKey && this.anchorId) {
      const range = rangePathsBetween(orderedIds, this.anchorId, scriptId);
      this.selectedIds = new Set(range);
      return false;
    }
    if (event && (event.metaKey || event.ctrlKey)) {
      const next = new Set(this.selectedIds);
      if (next.has(scriptId)) next.delete(scriptId);
      else next.add(scriptId);
      this.selectedIds = next;
      this.anchorId = scriptId;
      return false;
    }
    this.selectedIds = new Set([scriptId]);
    this.anchorId = scriptId;
    return true;
  }

  /**
   * Right-click / long-press: keep multi-selection if the target is already
   * selected; otherwise collapse to the clicked script.
   */
  prepareContextMenu(scriptId: string): string[] {
    if (this.selectedIds.has(scriptId) && this.selectedIds.size > 1) {
      return [...this.selectedIds];
    }
    this.selectedIds = new Set([scriptId]);
    this.anchorId = scriptId;
    return [scriptId];
  }
}

export const scriptLibrarySelection = new ScriptLibrarySelectionStore();
