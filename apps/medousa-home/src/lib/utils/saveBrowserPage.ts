/** Save / bookmark flows for the human browser. */

import { browserBookmarks } from "$lib/stores/browserBookmarks.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { toast } from "$lib/stores/toast.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { browserPageLabel } from "$lib/utils/browserUrl";
import {
  bookmarkNoteContent,
  findVaultNoteBySource,
  listVaultBrowserSaves,
  type VaultBrowserSave,
} from "$lib/utils/browserVaultSaves";

export type { VaultBrowserSave } from "$lib/utils/browserVaultSaves";

export type SaveToLibraryResult =
  | { status: "saved"; path: string }
  | { status: "already_saved"; path: string }
  | { status: "error"; message: string };

export type ToggleBookmarkResult = "added" | "removed";

export async function togglePageBookmark(
  url: string,
  title?: string | null,
): Promise<ToggleBookmarkResult> {
  const label = browserPageLabel(url, title);
  const added = browserBookmarks.toggle(url, label);
  return added ? "added" : "removed";
}

export async function savePageToLibrary(options: {
  url: string;
  title?: string | null;
  openNote?: boolean;
  spaceId?: string;
}): Promise<SaveToLibraryResult> {
  const url = options.url.trim();
  if (!url || url === "about:blank") {
    return { status: "error", message: "Nothing to save" };
  }

  const title = browserPageLabel(url, options.title);
  const existingPath = await findVaultNoteBySource(url);
  if (existingPath) {
    browserBookmarks.linkVaultPath(url, existingPath);
    return { status: "already_saved", path: existingPath };
  }

  const path = await vault.createNote({
    spaceId: options.spaceId ?? vault.activeSpace?.id ?? "other",
    title,
    content: bookmarkNoteContent(title, url),
    open: options.openNote ?? false,
  });

  if (!path) {
    const message = vault.error ?? "Could not save to Library";
    return { status: "error", message };
  }

  if (browserBookmarks.isBookmarked(url)) {
    browserBookmarks.linkVaultPath(url, path);
  } else {
    browserBookmarks.add(url, title, path);
  }

  return { status: "saved", path };
}

export async function loadSavedPages(): Promise<{
  quick: ReturnType<typeof browserBookmarks.list>;
  library: VaultBrowserSave[];
}> {
  await vault.refreshNotes();
  const library = await listVaultBrowserSaves(vault.notes);
  return {
    quick: browserBookmarks.list(),
    library,
  };
}

export function openSavedVaultNote(path: string) {
  if (layout.isMobile) {
    layout.openNotes({ view: "reader" });
  } else {
    layout.navigateDesktop("library");
  }
  void vault.openNote(path);
}

export function showSaveFeedback(
  result: SaveToLibraryResult,
  options?: { onOpenNote?: () => void },
) {
  if (result.status === "error") {
    toast.show(result.message);
    return;
  }

  const openNote = () => {
    if (options?.onOpenNote) {
      options.onOpenNote();
    } else {
      openSavedVaultNote(result.path);
    }
  };

  if (result.status === "already_saved") {
    toast.show("Already saved in Library", {
      actionLabel: "Open note",
      onAction: openNote,
    });
    return;
  }

  toast.show("Saved to Library", {
    actionLabel: "Open note",
    onAction: openNote,
  });
}

export function showBookmarkFeedback(result: ToggleBookmarkResult) {
  toast.show(result === "added" ? "Bookmarked" : "Bookmark removed");
}
