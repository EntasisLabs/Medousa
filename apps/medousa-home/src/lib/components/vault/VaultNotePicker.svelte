<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { fuzzyMatchVaultNotes } from "$lib/utils/vaultFuzzyMatch";
  import VaultKindBadge from "./VaultKindBadge.svelte";

  interface Props {
    open: boolean;
    onSelect: (path: string) => void;
    onClose: () => void;
  }

  let { open, onSelect, onClose }: Props = $props();

  let query = $state("");
  let highlightIndex = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  const labelByPath = $derived(vault.labelByPathMap);
  const matches = $derived(
    fuzzyMatchVaultNotes(vault.notes, query, labelByPath, 30),
  );

  $effect(() => {
    if (open) {
      query = "";
      highlightIndex = 0;
      void Promise.resolve().then(() => inputEl?.focus());
    }
  });

  $effect(() => {
    query;
    if (highlightIndex >= matches.length) {
      highlightIndex = Math.max(0, matches.length - 1);
    }
  });

  function choose(path: string) {
    onSelect(path);
    onClose();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (matches.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      highlightIndex = (highlightIndex + 1) % matches.length;
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      highlightIndex = (highlightIndex - 1 + matches.length) % matches.length;
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const note = matches[highlightIndex];
      if (note) choose(note.path);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div
    class="vault-note-picker-backdrop fixed inset-0 z-[90] flex items-start justify-center bg-surface-950/70 p-4 pt-[12vh]"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div
      class="vault-note-picker-panel w-full max-w-lg overflow-hidden rounded-xl border border-surface-700/60 bg-surface-900 shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-label="Pick a note"
    >
      <input
        bind:this={inputEl}
        class="w-full border-0 border-b border-surface-800 bg-transparent px-4 py-3 text-sm text-surface-50 outline-none"
        placeholder="Search notes…"
        bind:value={query}
      />
      <ul class="max-h-80 overflow-y-auto py-1">
        {#each matches as note, index (note.path)}
          <li>
            <button
              type="button"
              class="flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm {index === highlightIndex
                ? 'bg-primary-500/15 text-primary-100'
                : 'text-surface-100 hover:bg-surface-800/80'}"
              onclick={() => choose(note.path)}
            >
              <span class="min-w-0 flex-1">
                <span class="block truncate font-medium">
                  {labelByPath.get(note.path) ?? vaultDisplayTitle(note.title, note.path)}
                </span>
                <span class="workshop-faint block truncate font-mono text-[11px]">{note.path}</span>
              </span>
              <VaultKindBadge kind={note.kind} path={note.path} compact />
            </button>
          </li>
        {:else}
          <li class="px-4 py-8 text-center text-sm text-surface-500">No matching notes</li>
        {/each}
      </ul>
    </div>
  </div>
{/if}
