<script lang="ts">
  import { onMount } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import VaultTree from "./VaultTree.svelte";
  import VaultEditor from "./VaultEditor.svelte";

  interface Props {
    visible: boolean;
  }

  let { visible }: Props = $props();

  onMount(() => {
    (async () => {
      await vault.refreshNotes();
      if (vault.selectedPath) {
        await vault.openNote(vault.selectedPath);
      }
    })();
  });

  function handleSearchInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    void vault.runSearch(value);
  }
</script>

<section class="flex h-full min-w-0 flex-1 {visible ? '' : 'hidden'}">
  <SplitPane
    width={layout.vaultTreeWidth}
    side="left"
    min={180}
    max={420}
    onResize={(width) => layout.setVaultTreeWidth(width)}
  >
    <aside
      class="workshop-drawer flex h-full w-full flex-col border-r-2"
      aria-label="Vault browser"
    >
      <div class="workshop-header p-2">
        <input
          class="input text-sm"
          type="search"
          placeholder="Search vault…"
          value={vault.searchQuery}
          oninput={handleSearchInput}
        />
      </div>

      {#if vault.searchHits.length > 0}
        <ul class="max-h-40 overflow-y-auto border-b border-surface-500/45 p-2 text-sm">
          {#each vault.searchHits as hit (hit.note.path)}
            <li>
              <button
                type="button"
                class="w-full rounded-container-token px-2 py-1 text-left hover:bg-surface-700/80"
                onclick={() => vault.openNote(hit.note.path)}
              >
                <span class="font-medium">{hit.note.title}</span>
                <span class="workshop-faint block truncate">{hit.note.path}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <VaultTree
        tree={vault.tree}
        selectedPath={vault.selectedPath}
        labelByPath={vault.labelByPath()}
        onSelect={(path) => vault.openNote(path)}
      />
    </aside>
  </SplitPane>

  <VaultEditor visible={true} />
</section>
