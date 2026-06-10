<script lang="ts">
  import { onMount } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import VaultTree from "./VaultTree.svelte";
  import VaultEditor from "./VaultEditor.svelte";
  import VaultNewNoteDialog from "./VaultNewNoteDialog.svelte";
  import VaultSpaceChips from "./VaultSpaceChips.svelte";
  import VaultKindBadge from "./VaultKindBadge.svelte";

  interface Props {
    visible: boolean;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible, onOpenChat, onOpenWork, onSelectCard }: Props = $props();

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
      <div class="vault-browser-chrome shrink-0 border-b border-surface-500/45 bg-surface-800/50">
        <div class="px-3 pt-3">
          <input
            class="input w-full text-sm"
            type="search"
            placeholder="Search vault…"
            value={vault.searchQuery}
            oninput={handleSearchInput}
          />
        </div>

        <div class="flex flex-wrap gap-2 px-3 pt-2.5">
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            onclick={() => void vault.createDailyNote()}
            disabled={vault.saving}
          >
            Daily
          </button>
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            onclick={() => void vault.createWeeklyReview()}
            disabled={vault.saving}
          >
            Weekly
          </button>
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            onclick={() => vault.openNewNoteDialog()}
          >
            New
          </button>
        </div>

        <div
          class="flex flex-wrap items-center gap-2 px-3 pb-2.5 pt-2"
          role="group"
          aria-label="Vault view options"
        >
          <button
            type="button"
            aria-pressed={vault.showAgentReviewFilter}
            class="vault-filter-chip {vault.showAgentReviewFilter
              ? 'vault-filter-chip-active'
              : ''}"
            onclick={() => vault.setShowAgentReviewFilter(!vault.showAgentReviewFilter)}
          >
            Agent review
          </button>
          <button
            type="button"
            aria-pressed={vault.showSystemNotes}
            class="vault-filter-chip {vault.showSystemNotes ? 'vault-filter-chip-active' : ''}"
            onclick={() => vault.setShowSystemNotes(!vault.showSystemNotes)}
          >
            System notes
          </button>
        </div>

        <div class="border-t border-surface-500/35 px-3 py-2.5">
          <VaultSpaceChips embedded />
        </div>
      </div>

      {#if vault.searchHits.length > 0}
        <ul class="max-h-40 overflow-y-auto border-b border-surface-500/45 p-2 text-sm">
          {#each vault.searchHits as hit (hit.note.path)}
            <li>
              <button
                type="button"
                class="flex w-full items-center gap-2 rounded-container-token px-2 py-1 text-left hover:bg-surface-700/80"
                onclick={() => vault.openNote(hit.note.path)}
              >
                <span class="min-w-0 flex-1">
                  <span class="font-medium">{hit.note.title}</span>
                  <span class="workshop-faint block truncate">{hit.note.path}</span>
                </span>
                <VaultKindBadge kind={hit.note.kind} path={hit.note.path} compact />
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <VaultTree
        tree={vault.tree}
        selectedPath={vault.selectedPath}
        labelByPath={vault.labelByPath()}
        activeSpaceFilter={vault.activeSpaceFilter}
        onSelect={(path) => vault.openNote(path)}
      />
    </aside>
  </SplitPane>

  <VaultEditor
    visible={true}
    {onOpenChat}
    {onOpenWork}
    {onSelectCard}
  />
</section>

<VaultNewNoteDialog />
