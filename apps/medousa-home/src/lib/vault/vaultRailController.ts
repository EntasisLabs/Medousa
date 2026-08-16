import type { VaultLookupSnapshot } from "$lib/utils/vaultLookup";
import type { VaultNote, VaultTreeNode } from "$lib/types/vault";
import type { VaultNoteKind } from "$lib/utils/vaultFrontmatter";
import {
  flattenTreeNotePaths,
  rangePathsBetween,
} from "$lib/utils/vaultRailSelection";
import {
  completeGarageOnboarding,
  shouldShowGarageWizard,
} from "$lib/utils/garageOnboarding";
import { vaultOverlay } from "$lib/vault/vaultOverlay.svelte";
import type { LibraryBrowseMode } from "$lib/vault/vaultBrowseController";

export type VaultRailHost = {
  selectedPath: string | null;
  selectedPaths: Set<string>;
  selectionAnchorPath: string | null;
  libraryBrowseMode: LibraryBrowseMode;
  tree: VaultTreeNode[];
  treeExpandedByKey: Record<string, boolean>;
  lookupSnapshot: VaultLookupSnapshot;
  previewPresentation: "pane" | "panel";
  previewingAttachmentPath: string | null;
  garageWizardOpen: boolean;
  recentNotesList(limit?: number): VaultNote[];
  notesByKind(): { kind: VaultNoteKind; notes: VaultNote[] }[];
  scopedLibraryNotes(): VaultNote[];
};

export class VaultRailController {
  #host: VaultRailHost;

  constructor(host: VaultRailHost) {
    this.#host = host;
  }

  selectionAncestorSet(): Set<string> {
    return this.#host.lookupSnapshot.ancestorIdsForSelection;
  }

  isSelectionAncestor(pathOrFolder: string | null): boolean {
    if (!pathOrFolder) return false;
    return this.#host.lookupSnapshot.ancestorIdsForSelection.has(pathOrFolder);
  }

  treeExpandKeyFor(node: {
    path?: string | null;
    spaceId?: string | null;
    dropPrefix?: string | null;
    name: string;
  }): string {
    return (node.path ?? node.spaceId ?? node.dropPrefix ?? node.name).trim();
  }

  isTreeExpanded(key: string): boolean | undefined {
    const normalized = key.trim();
    if (!normalized) return undefined;
    if (Object.prototype.hasOwnProperty.call(this.#host.treeExpandedByKey, normalized)) {
      return this.#host.treeExpandedByKey[normalized];
    }
    return undefined;
  }

  setTreeExpanded(key: string, expanded: boolean) {
    const normalized = key.trim();
    if (!normalized) return;
    this.#host.treeExpandedByKey = {
      ...this.#host.treeExpandedByKey,
      [normalized]: expanded,
    };
  }

  isRailPathSelected(path: string): boolean {
    if (this.#host.selectedPaths.size > 0) return this.#host.selectedPaths.has(path);
    return this.#host.selectedPath === path;
  }

  clearRailSelection() {
    this.#host.selectedPaths = new Set();
    this.#host.selectionAnchorPath = null;
  }

  railNoteOrder(): string[] {
    switch (this.#host.libraryBrowseMode) {
      case "recent":
        return this.#host.recentNotesList(200).map((note) => note.path);
      case "kind":
        return this.#host.notesByKind().flatMap((group) =>
          group.notes.map((note) => note.path),
        );
      case "tags":
        return this.#host.scopedLibraryNotes().map((note) => note.path);
      case "folders":
      default:
        return flattenTreeNotePaths(this.#host.tree);
    }
  }

  applyRailSelection(path: string, event?: MouseEvent | null): boolean {
    const ordered = this.railNoteOrder();
    if (event?.shiftKey && this.#host.selectionAnchorPath) {
      const range = rangePathsBetween(ordered, this.#host.selectionAnchorPath, path);
      this.#host.selectedPaths = new Set(range);
      return false;
    }
    if (event && (event.metaKey || event.ctrlKey)) {
      const next = new Set(this.#host.selectedPaths);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      if (next.size === 0 && this.#host.selectedPath) {
        next.add(this.#host.selectedPath);
      }
      this.#host.selectedPaths = next;
      this.#host.selectionAnchorPath = path;
      return false;
    }
    this.#host.selectedPaths = new Set([path]);
    this.#host.selectionAnchorPath = path;
    return true;
  }

  prepareRailContextMenu(path: string): string[] {
    if (this.#host.selectedPaths.has(path) && this.#host.selectedPaths.size > 1) {
      return [...this.#host.selectedPaths];
    }
    this.#host.selectedPaths = new Set([path]);
    this.#host.selectionAnchorPath = path;
    return [path];
  }

  syncAttachmentPanelOverlay() {
    vaultOverlay.attachmentPanelOpen =
      this.#host.previewPresentation === "panel" &&
      Boolean(this.#host.previewingAttachmentPath);
  }

  openGarageWizard() {
    this.#host.garageWizardOpen = true;
  }

  closeGarageWizard() {
    this.#host.garageWizardOpen = false;
  }

  finishGarageOnboarding() {
    completeGarageOnboarding();
    this.#host.garageWizardOpen = false;
  }

  shouldPromptGarageWizard(): boolean {
    return shouldShowGarageWizard();
  }
}
