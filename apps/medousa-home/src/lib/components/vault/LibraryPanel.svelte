<script lang="ts">
  import { onMount } from "svelte";
  import { PanelLeftClose } from "@lucide/svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import VaultTree from "./VaultTree.svelte";
  import VaultEditor from "./VaultEditor.svelte";
  import VaultNewNoteDialog from "./VaultNewNoteDialog.svelte";
  import VaultSpaceChips from "./VaultSpaceChips.svelte";
  import VaultKindBadge from "./VaultKindBadge.svelte";
  import ExternalFilesBrowser from "./ExternalFilesBrowser.svelte";
  import VaultGarageImportWizard from "./VaultGarageImportWizard.svelte";
  import VaultSidebarCollapsedStrip from "./VaultSidebarCollapsedStrip.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { shouldShowGarageWizard } from "$lib/utils/garageOnboarding";

  interface Props {
    visible: boolean;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible, onOpenChat, onOpenWork, onSelectCard }: Props = $props();

  const externalHits = $derived(externalDesk.searchHits());
  const showVaultChrome = $derived(externalDesk.sidebarMode === "vault");

  onMount(() => {
    (async () => {
      await vault.refreshNotes();
      if (vault.selectedPath) {
        await vault.openNote(vault.selectedPath);
      }
      if (externalDesk.sidebarMode === "files" && externalDesk.pinnedRoots.length > 0) {
        await externalDesk.refreshAllRoots();
      }
      if (shouldShowGarageWizard() && !vault.selectedPath) {
        vault.openGarageWizard();
      }
    })();
  });

  function handleSearchInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    vault.searchQuery = value;
    externalDesk.setSearchQuery(value);
    void vault.runSearch(value);
  }

  async function handleExternalSearchHit(path: string) {
    externalDesk.selectExternalPath(path);
    await openAttachmentPath(path);
  }

  function handleLinkExternalHit(path: string) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(path);
  }
</script>

<section class="flex h-full min-w-0 flex-1 {visible ? '' : 'hidden'}">
  {#if layout.vaultSidebarCollapsed}
    <VaultSidebarCollapsedStrip onExpand={() => layout.setVaultSidebarCollapsed(false)} />
  {:else}
    <SplitPane
      width={layout.vaultTreeWidth}
      side="left"
      min={180}
      max={420}
      onResize={(width) => layout.setVaultTreeWidth(width)}
    >
      <aside
        class="workshop-drawer flex h-full w-full flex-col border-r-2"
        aria-label="Library browser"
      >
        <div class="vault-browser-chrome shrink-0 border-b border-surface-500/45 bg-surface-800/50">
          <div class="flex items-start gap-1 border-b border-surface-500/35 px-2 pt-2">
            <div class="flex min-w-0 flex-1 gap-0 pl-1 pt-1">
              <button
                type="button"
                role="tab"
                aria-selected={externalDesk.sidebarMode === "vault"}
                class="vault-sidebar-tab {externalDesk.sidebarMode === 'vault'
                  ? 'vault-sidebar-tab-active'
                  : ''}"
                onclick={() => externalDesk.setSidebarMode("vault")}
              >
                Vault
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={externalDesk.sidebarMode === "files"}
                class="vault-sidebar-tab {externalDesk.sidebarMode === 'files'
                  ? 'vault-sidebar-tab-active'
                  : ''}"
                onclick={() => externalDesk.setSidebarMode("files")}
              >
                Your files
              </button>
            </div>
            <button
              type="button"
              class="btn btn-xs variant-ghost-surface mt-1 shrink-0"
              title="Hide library browser"
              aria-label="Hide library browser"
              onclick={() => layout.setVaultSidebarCollapsed(true)}
            >
              <PanelLeftClose size={14} strokeWidth={2} />
            </button>
          </div>

        <div class="px-3 pt-3">
          <input
            class="input w-full text-sm"
            type="search"
            placeholder={showVaultChrome ? "Search vault and your files…" : "Search pinned folders…"}
            value={vault.searchQuery}
            oninput={handleSearchInput}
          />
        </div>

        {#if showVaultChrome}
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
              Developer notes
            </button>
          </div>

          <div class="border-t border-surface-500/35 px-3 py-2.5">
            <VaultSpaceChips embedded />
          </div>
        {/if}
      </div>

      {#if vault.searchHits.length > 0 || externalHits.length > 0}
        <div class="max-h-44 overflow-y-auto border-b border-surface-500/45 p-2 text-sm">
          {#if vault.searchHits.length > 0}
            <p class="mb-1 px-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
              Vault
            </p>
            <ul>
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

          {#if externalHits.length > 0}
            <p class="mb-1 mt-2 px-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
              Your files
            </p>
            <ul>
              {#each externalHits as entry (entry.path)}
                <li>
                  <div class="flex items-center gap-1 rounded-container-token px-2 py-1 hover:bg-surface-700/80">
                    <button
                      type="button"
                      class="min-w-0 flex-1 text-left"
                      onclick={() => void handleExternalSearchHit(entry.path)}
                    >
                      <span class="font-medium">{entry.name}</span>
                      <span class="workshop-faint block truncate">{entry.path}</span>
                    </button>
                    {#if vault.selectedPath}
                      <button
                        type="button"
                        class="btn btn-xs variant-soft-primary"
                        onclick={() => handleLinkExternalHit(entry.path)}
                      >
                        Link
                      </button>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      {#if showVaultChrome}
        <VaultTree
          tree={vault.tree}
          selectedPath={vault.selectedPath}
          labelByPath={vault.labelByPath()}
          activeSpaceFilter={vault.activeSpaceFilter}
          onSelect={(path) => vault.openNote(path)}
        />
      {:else}
        <ExternalFilesBrowser />
      {/if}
    </aside>
    </SplitPane>
  {/if}

  <VaultEditor
    visible={true}
    {onOpenChat}
    {onOpenWork}
    {onSelectCard}
  />
</section>

<VaultNewNoteDialog />
<VaultGarageImportWizard />
