<script lang="ts">
  import { onMount, untrack } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import VaultTree from "./VaultTree.svelte";
  import VaultLibraryBrowseLists from "./VaultLibraryBrowseLists.svelte";
  import VaultEditor from "./VaultEditor.svelte";
  import VaultKindBadge from "./VaultKindBadge.svelte";
  import VaultNewNoteDialog from "./VaultNewNoteDialog.svelte";
  import ExternalFilesBrowser from "./ExternalFilesBrowser.svelte";
  import ExternalFileRow from "./ExternalFileRow.svelte";
  import ExternalFileLibraryPreview from "./ExternalFileLibraryPreview.svelte";
  import VaultNewGroupDialog from "./VaultNewGroupDialog.svelte";
  import VaultSidebarCollapsedStrip from "./VaultSidebarCollapsedStrip.svelte";
  import VaultLibraryChrome from "./VaultLibraryChrome.svelte";
  import ArtifactLibraryPanel from "$lib/components/artifacts/ArtifactLibraryPanel.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import type { ExternalFileEntry } from "$lib/types/externalDesk";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";

  interface Props {
    visible: boolean;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
    /** Hosted inside LME — hide library tabs; open notes as workspace tabs. */
    lmeHosted?: boolean;
  }

  let {
    visible,
    onOpenChat,
    onOpenWork,
    onSelectCard,
    lmeHosted = false,
  }: Props = $props();

  async function openNote(path: string) {
    if (lmeHosted) {
      await lmeWorkspace.openNote(path);
      return;
    }
    await vault.openNote(path);
  }

  const externalHits = $derived(externalDesk.searchHitsList);
  const showVaultChrome = $derived(externalDesk.sidebarMode === "vault");
  const showYourFiles = $derived(externalDesk.sidebarMode === "files");
  const showPresentations = $derived(externalDesk.sidebarMode === "presentations");
  const showFilesSearch = $derived(
    showYourFiles && vault.searchQuery.trim().length > 0,
  );
  const searchingNotes = $derived(
    showVaultChrome && vault.searchQuery.trim().length > 0,
  );
  const canLinkFiles = $derived(Boolean(vault.selectedPath));

  onMount(() => {
    let cancelled = false;
    (async () => {
      await vault.refreshVaultRoots();
      if (cancelled) return;
      await vault.refreshNotes();
      if (cancelled) return;
      if (vault.selectedPath) {
        if (lmeHosted) {
          // Hydrate only — never force explorer mode (remount races yank Scripts → Notes).
          await lmeWorkspace.openNote(vault.selectedPath, { activateMode: false });
        } else {
          await vault.openNote(vault.selectedPath);
        }
      }
      if (cancelled) return;
      if (externalDesk.sidebarMode === "files" && externalDesk.pinnedRoots.length > 0) {
        await externalDesk.refreshAllRoots();
      }
      if (cancelled) return;
    })();
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    if (!lmeHosted || !visible) return;
    // Subscribe to label map; refresh must not track `lmeWorkspace.tabs` (write loop).
    void vault.labelByPathMap;
    untrack(() => lmeWorkspace.refreshNoteTitles());
  });

  function handleExternalSearch(query: string) {
    vault.searchQuery = query;
    externalDesk.setSearchQuery(query);
  }

  async function handleExternalOpen(entry: ExternalFileEntry) {
    externalDesk.selectExternalPath(entry.path);
    const attachment = externalDesk.attachmentForPath(entry.path);
    if (canPreviewAttachment(attachment)) {
      vault.previewAttachment(entry.path, "pane");
      return;
    }
    await openAttachmentPath(entry.path);
  }

  function handleLinkExternalHit(entry: ExternalFileEntry) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(entry.path);
  }
</script>

<section class="flex h-full min-w-0 flex-1 {visible ? '' : 'hidden'}">
  {#if showPresentations}
    <div class="flex h-full min-w-0 flex-1 flex-col">
      <VaultLibraryChrome showVaultChrome={false} hideLibraryTabs={lmeHosted} />
      <ArtifactLibraryPanel {onOpenChat} />
    </div>
  {:else if showYourFiles}
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
          aria-label="External files browser"
        >
          <VaultLibraryChrome
            showVaultChrome={false}
            hideLibraryTabs={lmeHosted}
            onSearchExternal={handleExternalSearch}
          />

          {#if showFilesSearch}
            <div class="flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
              <p class="mb-2 px-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
                Search results
              </p>
              {#if externalHits.length === 0}
                <p class="px-2 py-4 text-sm text-surface-500">No matches in pinned folders.</p>
              {:else}
                <ul class="space-y-0.5">
                  {#each externalHits as entry (entry.path)}
                    <li>
                      <ExternalFileRow
                        {entry}
                        selected={externalDesk.selectedExternalPath === entry.path}
                        showLink={canLinkFiles}
                        onOpen={handleExternalOpen}
                        onLink={handleLinkExternalHit}
                      />
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          {:else}
            <ExternalFilesBrowser />
          {/if}
        </aside>
      </SplitPane>
    {/if}
    <ExternalFileLibraryPreview />
  {:else if layout.vaultSidebarCollapsed}
    <VaultSidebarCollapsedStrip onExpand={() => layout.setVaultSidebarCollapsed(false)} />
    <VaultEditor
      visible={true}
      {onOpenChat}
      {onOpenWork}
      {onSelectCard}
    />
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
        <VaultLibraryChrome
          showVaultChrome={true}
          hideLibraryTabs={lmeHosted}
          onSearchExternal={handleExternalSearch}
        />

        {#if vault.error}
          <p class="mx-2 mb-2 rounded-container-token border border-error-500/30 bg-error-500/10 px-2 py-1.5 text-xs text-error-300">
            {vault.error}
          </p>
        {/if}

        {#if searchingNotes}
          {#if vault.searchHits.length === 0}
            <p class="workshop-muted px-3 py-4 text-xs">No notes match.</p>
          {:else}
            <ul class="min-h-0 flex-1 divide-y divide-surface-500/35 overflow-y-auto">
              {#each vault.searchHits as hit (hit.note.path)}
                <li>
                  <button
                    type="button"
                    class="flex w-full items-center gap-2 px-3 py-2 text-left transition hover:bg-surface-800/70 {vault.selectedPath ===
                    hit.note.path
                      ? 'workshop-list-row-active'
                      : ''}"
                    onclick={() => void openNote(hit.note.path)}
                  >
                    <span class="min-w-0 flex-1">
                      <span class="block truncate text-sm font-medium text-surface-100">
                        {vault.labelByPathMap.get(hit.note.path) ??
                          vaultDisplayTitle(hit.note.title, hit.note.path)}
                      </span>
                      <span class="workshop-faint mt-0.5 block truncate font-mono text-[10px]">
                        {hit.note.path}
                      </span>
                    </span>
                    <VaultKindBadge kind={hit.note.kind} path={hit.note.path} compact />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {:else if vault.libraryBrowseMode === "folders"}
          <VaultTree
            tree={vault.tree}
            selectedPath={vault.selectedPath}
            labelByPath={vault.labelByPathMap}
            activeSpaceFilter={vault.activeSpaceFilter}
            onSelect={(path) => void openNote(path)}
            onMoveNote={(sourcePath, targetPrefix) => {
              void vault.moveNoteToFolder(sourcePath, targetPrefix);
            }}
          />
        {:else}
          <VaultLibraryBrowseLists onSelect={(path) => void openNote(path)} />
        {/if}
      </aside>
    </SplitPane>
    <VaultEditor
      visible={true}
      {onOpenChat}
      {onOpenWork}
      {onSelectCard}
    />
  {/if}
</section>

<VaultNewNoteDialog />
<VaultNewGroupDialog />
