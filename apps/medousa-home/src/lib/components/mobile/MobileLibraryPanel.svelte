<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ChevronLeft, MessageCircle, MoreHorizontal } from "@lucide/svelte";
  import VaultTree from "$lib/components/vault/VaultTree.svelte";
  import VaultEditor from "$lib/components/vault/VaultEditor.svelte";
  import VaultSpaceChips from "$lib/components/vault/VaultSpaceChips.svelte";
  import VaultKindBadge from "$lib/components/vault/VaultKindBadge.svelte";
  import VaultNewNoteDialog from "$lib/components/vault/VaultNewNoteDialog.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { prepareTalkAboutNote } from "$lib/utils/vaultNoteBridge";
  import {
    bindVaultLongPress,
    handleVaultNoteContextMenuEvent,
    shouldSuppressVaultContextMenuClick,
  } from "$lib/utils/vaultContextMenuEvents";

  interface Props {
    visible: boolean;
    onOpenChat?: () => void | Promise<void>;
  }

  let { visible, onOpenChat }: Props = $props();

  let listScrollEl = $state<HTMLDivElement | null>(null);

  const view = $derived(layout.libraryView);

  onMount(() => {
    void vault.refreshNotes();
  });

  $effect(() => {
    if (!visible || view !== "list" || !listScrollEl) return;
    void tick().then(() => {
      if (listScrollEl) {
        listScrollEl.scrollTop = layout.libraryListScrollTop;
      }
    });
  });

  function handleSearchInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    void vault.runSearch(value);
  }

  function handleListScroll(event: Event) {
    layout.setLibraryListScrollTop((event.currentTarget as HTMLDivElement).scrollTop);
  }

  async function openNote(path: string) {
    if (shouldSuppressVaultContextMenuClick()) return;
    await vault.openNote(path);
    vault.enterPreviewMode();
    layout.setLibraryView("reader");
  }

  function backToList() {
    layout.setLibraryView("list");
  }

  const readerTitle = $derived(
    vault.selectedPath
      ? (vault.labelByPath().get(vault.selectedPath) ??
        vaultDisplayTitle(vault.title, vault.selectedPath))
      : "Note",
  );

  const saveWhisper = $derived(vault.saveWhisper());

  async function handleTalkAboutNote() {
    if (!vault.selectedPath || !onOpenChat) return;
    if (vault.dirty) await vault.flushSave();
    const { scope, draft } = prepareTalkAboutNote(
      vault.selectedPath,
      vault.title,
      vault.content,
      vault.wikilinksOut,
      vault.backlinks,
    );
    chat.prefillFromVaultNote(scope, draft);
    await onOpenChat();
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if view === "list"}
    <div
      bind:this={listScrollEl}
      class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto"
      onscroll={handleListScroll}
    >
      <div class="space-y-2 border-b border-surface-500/40 p-3">
        <input
          class="input w-full text-sm"
          type="search"
          placeholder="Search notes…"
          value={vault.searchQuery}
          oninput={handleSearchInput}
        />
        <div class="flex gap-2">
          <button
            type="button"
            class="btn btn-sm flex-1 variant-filled-primary"
            onclick={() => void vault.createDailyNote()}
          >
            Daily
          </button>
          <button
            type="button"
            class="btn btn-sm flex-1 variant-soft-surface"
            onclick={() => vault.openNewNoteDialog()}
          >
            New
          </button>
        </div>
        <VaultSpaceChips compact />
      </div>

      {#if vault.searchHits.length > 0}
        <ul class="border-b border-surface-500/40 p-2">
          {#each vault.searchHits as hit (hit.note.path)}
            <li>
              <button
                type="button"
                class="mobile-you-row flex w-full items-center gap-2 text-left"
                onclick={() => void openNote(hit.note.path)}
                oncontextmenu={(event) =>
                  handleVaultNoteContextMenuEvent(hit.note.path, event)}
                use:bindVaultLongPress={() => hit.note.path}
              >
                <span class="min-w-0 flex-1">
                  <span class="font-medium text-surface-100">{hit.note.title}</span>
                  <span class="workshop-faint block truncate text-xs">{hit.note.path}</span>
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
      <p class="min-w-0 flex-1 truncate text-sm font-medium text-surface-100">{readerTitle}</p>
      {#if saveWhisper}
        <span class="shrink-0 text-xs text-surface-400">{saveWhisper}</span>
      {/if}
      {#if vault.selectedPath}
        {#if onOpenChat}
          <button
            type="button"
            class="mobile-icon-btn shrink-0 text-primary-300"
            aria-label="Talk about this note"
            title="Talk about this note"
            disabled={vault.noteLoading}
            onclick={() => void handleTalkAboutNote()}
          >
            <MessageCircle size={18} strokeWidth={1.75} />
          </button>
        {/if}
        <button
          type="button"
          class="btn btn-sm shrink-0 {vault.editorMode === 'edit'
            ? 'variant-soft-primary'
            : 'variant-ghost-surface'}"
          onclick={() =>
            vault.editorMode === "edit"
              ? vault.enterPreviewMode()
              : vault.enterEditMode()}
        >
          {vault.editorMode === "edit" ? "Preview" : "Edit"}
        </button>
        <button
          type="button"
          class="mobile-icon-btn shrink-0"
          aria-label="Note actions"
          onclick={() => vault.openNoteActions()}
        >
          <MoreHorizontal size={18} strokeWidth={1.75} />
        </button>
      {/if}
    </header>
    <VaultEditor visible={true} mobile={true} />
  {/if}
</section>

<VaultNewNoteDialog />
