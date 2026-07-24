<script lang="ts">
  import { RotateCcw, Trash2, X } from "@lucide/svelte";
  import { listVaultTrash, restoreVaultTrash, type VaultTrashEntry } from "$lib/daemon";
  import { vault } from "$lib/stores/vault.svelte";

  interface Props {
    open?: boolean;
    onClose?: () => void;
  }

  let { open = false, onClose }: Props = $props();

  let entries = $state<VaultTrashEntry[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    void refresh();
  });

  async function refresh() {
    busy = true;
    error = null;
    try {
      const result = await listVaultTrash(80);
      entries = result.entries ?? [];
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function restore(path: string) {
    busy = true;
    error = null;
    try {
      await restoreVaultTrash(path);
      await vault.refreshNotes();
      await vault.openNote(path, { skipLeaveFlush: true });
      await refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if open}
  <div
    class="vault-trash-panel fixed inset-y-0 right-0 z-40 flex w-[min(22rem,100%)] flex-col border-l border-surface-500/40 bg-surface-950/95 shadow-xl"
    role="dialog"
    aria-label="Trash"
  >
    <header class="flex items-center justify-between gap-2 border-b border-surface-500/30 px-3 py-2">
      <div class="flex min-w-0 items-center gap-2">
        <Trash2 class="h-4 w-4 text-surface-300" />
        <h2 class="text-sm font-semibold text-surface-50">Trash</h2>
      </div>
      <button
        type="button"
        class="rounded p-1 text-surface-300 hover:bg-surface-800"
        aria-label="Close trash"
        onclick={() => onClose?.()}
      >
        <X class="h-4 w-4" />
      </button>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto p-3">
      {#if busy && entries.length === 0}
        <p class="workshop-faint text-xs">Loading…</p>
      {:else if entries.length === 0}
        <p class="workshop-faint text-xs">Trash is empty.</p>
      {:else}
        <ul class="space-y-1.5">
          {#each entries as entry (entry.path)}
            <li class="rounded border border-surface-500/25 bg-surface-900/60 px-2 py-1.5">
              <p class="truncate text-sm text-surface-100">{entry.path}</p>
              {#if entry.trashedAt}
                <p class="workshop-faint text-[11px]">
                  {new Date(entry.trashedAt).toLocaleString()}
                </p>
              {/if}
              <button
                type="button"
                class="mt-1 inline-flex items-center gap-1 text-[11px] text-surface-200 underline-offset-2 hover:underline"
                disabled={busy}
                onclick={() => void restore(entry.path)}
              >
                <RotateCcw class="h-3 w-3" />
                Restore
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if error}
        <p class="mt-2 text-xs text-rose-300/90" role="alert">{error}</p>
      {/if}
    </div>
  </div>
{/if}
