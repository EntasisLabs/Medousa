<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";

  let titleDraft = $state("");
  let pathDraft = $state("");
  let confirmDelete = $state(false);

  $effect(() => {
    if (vault.noteActionsOpen) {
      titleDraft = vault.title;
      pathDraft = vault.selectedPath ?? "";
      confirmDelete = false;
    }
  });

  async function handleRenameTitle(event: Event) {
    event.preventDefault();
    if (!titleDraft.trim()) return;
    await vault.renameNoteTitle(titleDraft.trim());
    vault.closeNoteActions();
  }

  async function handleMovePath(event: Event) {
    event.preventDefault();
    if (!pathDraft.trim()) return;
    await vault.relocateNote(pathDraft.trim());
    vault.closeNoteActions();
  }

  async function handleDelete() {
    if (!vault.selectedPath) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    const path = vault.selectedPath;
    vault.closeNoteActions();
    await vault.archiveNote(path);
  }
</script>

{#if vault.noteActionsOpen && vault.selectedPath}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4"
    role="dialog"
    aria-modal="true"
    aria-labelledby="note-actions-title"
  >
    <div class="card w-full max-w-md space-y-4 p-5 shadow-xl">
      <div class="flex items-start justify-between gap-3">
        <h3 id="note-actions-title" class="text-base font-semibold">Note actions</h3>
        <button
          type="button"
          class="btn btn-xs variant-ghost-surface"
          aria-label="Close"
          onclick={() => vault.closeNoteActions()}
        >
          Close
        </button>
      </div>

      <form class="space-y-3" onsubmit={handleRenameTitle}>
        <label class="block space-y-1 text-left text-sm">
          <span class="text-surface-400">Title</span>
          <input class="input w-full" type="text" bind:value={titleDraft} />
        </label>
        <button
          type="submit"
          class="btn btn-sm variant-soft-primary"
          disabled={vault.saving || !titleDraft.trim()}
        >
          Rename title
        </button>
      </form>

      <form class="space-y-3 border-t border-surface-500/35 pt-4" onsubmit={handleMovePath}>
        <label class="block space-y-1 text-left text-sm">
          <span class="text-surface-400">File path</span>
          <input
            class="input w-full font-mono text-xs"
            type="text"
            bind:value={pathDraft}
            placeholder="projects/my-note.md"
          />
        </label>
        <p class="text-xs text-surface-500">
          Change folder or filename. You can also drag notes onto folders in the sidebar.
        </p>
        <button
          type="submit"
          class="btn btn-sm variant-soft-surface"
          disabled={vault.saving || !pathDraft.trim()}
        >
          Move / rename file
        </button>
      </form>

      <div class="border-t border-surface-500/35 pt-4">
        <button
          type="button"
          class="btn btn-sm {confirmDelete ? 'variant-filled-error' : 'variant-soft-error'}"
          disabled={vault.saving}
          onclick={() => void handleDelete()}
        >
          {confirmDelete ? "Confirm delete" : "Delete note"}
        </button>
        {#if confirmDelete}
          <p class="mt-2 text-xs text-surface-500">
            Moves the note to vault trash. Click again to confirm.
          </p>
        {/if}
      </div>
    </div>
  </div>
{/if}
