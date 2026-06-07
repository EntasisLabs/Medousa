<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronLeft } from "@lucide/svelte";
  import VaultTree from "$lib/components/vault/VaultTree.svelte";
  import VaultEditor from "$lib/components/vault/VaultEditor.svelte";
  import { vault } from "$lib/stores/vault.svelte";

  interface Props {
    visible: boolean;
  }

  let { visible }: Props = $props();

  let view = $state<"list" | "reader">("list");

  $effect(() => {
    if (!visible) {
      view = "list";
    }
  });

  onMount(() => {
    void vault.refreshNotes();
  });

  function handleSearchInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    void vault.runSearch(value);
  }

  async function openNote(path: string) {
    await vault.openNote(path);
    view = "reader";
  }

  function backToList() {
    view = "list";
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if view === "list"}
    <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto">
      <div class="border-b border-surface-500/40 p-3">
        <input
          class="input w-full text-sm"
          type="search"
          placeholder="Search notes…"
          value={vault.searchQuery}
          oninput={handleSearchInput}
        />
      </div>

      {#if vault.searchHits.length > 0}
        <ul class="border-b border-surface-500/40 p-2">
          {#each vault.searchHits as hit (hit.note.path)}
            <li>
              <button
                type="button"
                class="mobile-you-row w-full text-left"
                onclick={() => openNote(hit.note.path)}
              >
                <span class="font-medium text-surface-100">{hit.note.title}</span>
                <span class="workshop-faint block truncate text-xs">{hit.note.path}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <VaultTree
        tree={vault.tree}
        selectedPath={vault.selectedPath}
        labelByPath={vault.labelByPath()}
        onSelect={openNote}
      />
    </div>
  {:else}
    <header class="mobile-you-subheader flex items-center gap-2">
      <button
        type="button"
        class="mobile-icon-btn shrink-0"
        aria-label="Back to notes"
        onclick={backToList}
      >
        <ChevronLeft size={20} strokeWidth={1.75} />
      </button>
      <p class="min-w-0 truncate text-sm font-medium text-surface-100">Note</p>
    </header>
    <VaultEditor visible={true} mobile={true} />
  {/if}
</section>
