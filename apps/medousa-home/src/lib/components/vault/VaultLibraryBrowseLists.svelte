<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { kindLabel } from "$lib/utils/vaultFrontmatter";
  import { formatVaultRelativeTime } from "$lib/utils/vaultRecent";
  import {
    handleVaultNoteContextMenuEvent,
  } from "$lib/utils/vaultContextMenuEvents";
  import type { VaultNote } from "$lib/types/vault";
  import VaultKindBadge from "./VaultKindBadge.svelte";

  interface Props {
    onSelect: (path: string) => void;
  }

  let { onSelect }: Props = $props();

  let expandedTag = $state<string | null>(null);
  let expandedKinds = $state<Set<string>>(new Set(["daily", "project", "note"]));

  const tagRows = $derived(vault.vaultTags);
  const recentNotes = $derived(vault.recentNotesList());
  const kindGroups = $derived(vault.notesByKind());
  const labelByPath = $derived(vault.labelByPathMap);

  $effect(() => {
    if (vault.libraryBrowseMode === "tags" && vault.vaultTags.length === 0) {
      void vault.refreshVaultTags();
    }
  });

  function toggleTag(tag: string) {
    expandedTag = expandedTag === tag ? null : tag;
  }

  function toggleKind(kind: string) {
    const next = new Set(expandedKinds);
    if (next.has(kind)) next.delete(kind);
    else next.add(kind);
    expandedKinds = next;
  }

  function noteLabel(note: VaultNote): string {
    return labelByPath.get(note.path) ?? vaultDisplayTitle(note.title, note.path);
  }
</script>

<nav class="flex min-h-0 flex-1 flex-col overflow-y-auto px-1.5 py-1" aria-label="Vault browse">
  {#if vault.libraryBrowseMode === "tags"}
    {#if tagRows.length === 0}
      <p class="px-2 py-4 text-sm text-surface-400">No tags in this view yet.</p>
    {:else}
      {#each tagRows as row (row.tag)}
        <div>
          <button
            type="button"
            class="vault-tree-row flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {expandedTag ===
            row.tag
              ? 'bg-surface-700/50 text-surface-100'
              : 'text-surface-300'}"
            aria-expanded={expandedTag === row.tag}
            onclick={() => toggleTag(row.tag)}
          >
            <span class="workshop-faint flex w-4 shrink-0 items-center justify-center text-xs">
              {expandedTag === row.tag ? "▾" : "▸"}
            </span>
            <span class="min-w-0 flex-1 truncate">#{row.tag}</span>
            <span class="tabular-nums text-xs text-surface-500">{row.count}</span>
          </button>
          {#if expandedTag === row.tag}
            {#each vault.notesForTag(row.tag) as note (note.path)}
              <button
                type="button"
                class="vault-tree-row vault-tree-row--note flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {note.path ===
                vault.selectedPath
                  ? 'bg-primary-500/15 text-primary-300'
                  : 'text-surface-300'}"
                style="padding-left: 1.75rem"
                title={note.path}
                onclick={() => onSelect(note.path)}
                oncontextmenu={(event) =>
                  handleVaultNoteContextMenuEvent(note.path, event)}
              >
                <span class="min-w-0 flex-1 truncate">{noteLabel(note)}</span>
                <VaultKindBadge kind={note.kind} path={note.path} compact />
              </button>
            {:else}
              <p class="px-2 py-1 pl-7 text-xs text-surface-500">No notes with this tag.</p>
            {/each}
          {/if}
        </div>
      {/each}
    {/if}
  {:else if vault.libraryBrowseMode === "recent"}
    {#if recentNotes.length === 0}
      <p class="px-2 py-4 text-sm text-surface-400">No recent notes yet.</p>
    {:else}
      {#each recentNotes as note (note.path)}
        <button
          type="button"
          class="vault-tree-row vault-tree-row--note flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {note.path ===
          vault.selectedPath
            ? 'bg-primary-500/15 text-primary-300'
            : 'text-surface-300'}"
          title={note.path}
          onclick={() => onSelect(note.path)}
          oncontextmenu={(event) => handleVaultNoteContextMenuEvent(note.path, event)}
        >
          <span class="min-w-0 flex-1 truncate">{noteLabel(note)}</span>
          <span class="shrink-0 text-[10px] tabular-nums text-surface-500">
            {formatVaultRelativeTime(note.modified_at_utc)}
          </span>
          <VaultKindBadge kind={note.kind} path={note.path} compact />
        </button>
      {/each}
    {/if}
  {:else if vault.libraryBrowseMode === "kind"}
    {#if kindGroups.length === 0}
      <p class="px-2 py-4 text-sm text-surface-400">No notes in this view yet.</p>
    {:else}
      {#each kindGroups as group (group.kind)}
        <div>
          <button
            type="button"
            class="vault-tree-row flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {expandedKinds.has(
              group.kind,
            )
              ? 'bg-surface-700/50 text-surface-100'
              : 'text-surface-300'}"
            aria-expanded={expandedKinds.has(group.kind)}
            onclick={() => toggleKind(group.kind)}
          >
            <span class="workshop-faint flex w-4 shrink-0 items-center justify-center text-xs">
              {expandedKinds.has(group.kind) ? "▾" : "▸"}
            </span>
            <span class="min-w-0 flex-1 truncate">{kindLabel(group.kind)}</span>
            <span class="tabular-nums text-xs text-surface-500">{group.notes.length}</span>
          </button>
          {#if expandedKinds.has(group.kind)}
            {#each group.notes as note (note.path)}
              <button
                type="button"
                class="vault-tree-row vault-tree-row--note flex w-full items-center gap-1.5 rounded-container-token px-2 py-1 text-left text-sm outline-none hover:bg-surface-700/80 focus-visible:ring-1 focus-visible:ring-primary-400/50 {note.path ===
                vault.selectedPath
                  ? 'bg-primary-500/15 text-primary-300'
                  : 'text-surface-300'}"
                style="padding-left: 1.75rem"
                title={note.path}
                onclick={() => onSelect(note.path)}
                oncontextmenu={(event) =>
                  handleVaultNoteContextMenuEvent(note.path, event)}
              >
                <span class="min-w-0 flex-1 truncate">{noteLabel(note)}</span>
              </button>
            {/each}
          {/if}
        </div>
      {/each}
    {/if}
  {/if}
</nav>
