<script lang="ts">
  import { onMount } from "svelte";
  import { FileDown, MoreHorizontal, PanelLeftOpen } from "@lucide/svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { noteWorkshop } from "$lib/stores/noteWorkshop.svelte";
  import { vaultBreadcrumb, vaultDisplayTitle } from "$lib/utils/formatVault";
  import { formatCardTitle } from "$lib/utils/formatWork";
  import {
    buildAskAboutNoteDraft,
    buildWorkAskFromNote,
    prepareTalkAboutNote,
  } from "$lib/utils/vaultNoteBridge";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";
  import { findLedgerTable } from "$lib/utils/markdownTable";
  import { findKanbanBoard, noteHasKanbanBoard } from "$lib/utils/markdownKanban";
  import VaultEmptyState from "./VaultEmptyState.svelte";
  import VaultKindBadge from "./VaultKindBadge.svelte";
  import LedgerTableEditor from "./LedgerTableEditor.svelte";
  import KanbanBoardEditor from "./KanbanBoardEditor.svelte";
  import VaultMarkdownPreview from "./VaultMarkdownPreview.svelte";
  import VaultNoteLinksPanel from "./VaultNoteLinksPanel.svelte";
  import VaultConflictBar from "./VaultConflictBar.svelte";
  import VaultProposalBar from "./VaultProposalBar.svelte";
  import VaultMarkdownEditor from "./VaultMarkdownEditor.svelte";
  import VaultNoteActionsMenu from "./VaultNoteActionsMenu.svelte";
  import VaultAttachmentBar from "./VaultAttachmentBar.svelte";
  import VaultAttachmentPreview from "./VaultAttachmentPreview.svelte";
  import { exportVaultNotePdf } from "$lib/utils/vaultPdfExport";

  interface Props {
    visible: boolean;
    /** Mobile reader: preview-only, no edit chrome. */
    mobile?: boolean;
    onOpenChat?: () => void;
    onOpenWork?: () => void;
    onSelectCard?: (id: string) => void | Promise<void>;
  }

  let { visible, mobile = false, onOpenChat, onOpenWork, onSelectCard }: Props = $props();

  let exportingPdf = $state(false);

  const displayTitle = $derived(
    vault.selectedPath
      ? (vault.labelByPath().get(vault.selectedPath) ??
        vaultDisplayTitle(vault.title, vault.selectedPath))
      : "Library",
  );

  const breadcrumb = $derived(
    vault.selectedPath ? vaultBreadcrumb(vault.selectedPath) : null,
  );

  const activeSpace = $derived(vault.activeSpace);
  const SpaceIcon = $derived(
    activeSpace ? iconForSpace(activeSpace.id) : null,
  );

  const labelByPath = $derived(vault.labelByPath());
  const hasLedgerTable = $derived(Boolean(findLedgerTable(vault.content)));
  const hasKanbanBoard = $derived(noteHasKanbanBoard(vault.content));
  const kanbanBoard = $derived(hasKanbanBoard ? findKanbanBoard(vault.content) : null);

  const showLedgerTable = $derived(
    !mobile &&
      vault.editorMode === "edit" &&
      vault.selectedKind === "ledger" &&
      vault.ledgerEditMode === "table" &&
      hasLedgerTable,
  );

  const showKanbanBoard = $derived(
    vault.editorMode === "edit" &&
      hasKanbanBoard &&
      vault.boardEditMode === "board" &&
      kanbanBoard !== null,
  );

  const showMarkdownEditor = $derived(
    vault.editorMode === "edit" && !showLedgerTable && !showKanbanBoard,
  );

  const showSplitEditor = $derived(
    !mobile &&
      showMarkdownEditor &&
      layout.vaultSplitEnabled &&
      (!vault.isWriteFirstKind || vault.isAuthoringSource),
  );

  const editorSurface = $derived(
    vault.isWriteFirstKind && !vault.isAuthoringSource ? "write" : "source",
  );

  const showPreviewOnly = $derived(
    vault.editorMode === "preview" ||
      (!showMarkdownEditor && !showLedgerTable),
  );

  const showLinksPanel = $derived(
    !mobile &&
      layout.vaultLinksPanelOpen &&
      vault.selectedPath &&
      (vault.wikilinksOut.length > 0 || vault.backlinks.length > 0),
  );

  const previewFirstKind = $derived(
    !vault.isWriteFirstKind &&
      (vault.selectedKind === "daily" || vault.selectedKind === "note"),
  );

  const linkedWork = $derived(
    vault.selectedPath && !mobile
      ? workspace.inMotionCardsForVaultPath(vault.selectedPath)
      : [],
  );

  $effect(() => {
    if (vault.selectedPath && !mobile) {
      void workspace.prefetchVaultLinkedWork(vault.selectedPath);
    }
  });

  async function handleAskAboutNote() {
    if (!vault.selectedPath) return;
    if (vault.dirty) await vault.flushSave();
    const { scope, draft } = prepareTalkAboutNote(
      vault.selectedPath,
      vault.title,
      vault.content,
      vault.wikilinksOut,
      vault.backlinks,
    );
    chat.prefillFromVaultNote(scope, draft, { pin: true });
    void chat.ensureSessionHydrated();

    if (!mobile) {
      noteWorkshop.openForNote(vault.selectedPath);
      return;
    }

    if (!onOpenChat) return;
    onOpenChat();
  }

  async function handleAskInChatTab() {
    if (!vault.selectedPath || !onOpenChat) return;
    if (vault.dirty) await vault.flushSave();
    const { scope, draft } = prepareTalkAboutNote(
      vault.selectedPath,
      vault.title,
      vault.content,
      vault.wikilinksOut,
      vault.backlinks,
    );
    chat.prefillFromVaultNote(scope, draft, { pin: true });
    onOpenChat();
  }

  async function handleSendToWork() {
    if (!vault.selectedPath || !onOpenWork) return;
    if (vault.dirty) await vault.flushSave();
    try {
      await workspace.submitAsk({
        prompt: buildWorkAskFromNote(
          vault.selectedPath,
          vault.title,
          vault.content,
        ),
      });
      onOpenWork();
    } catch {
      // workspace.submitAsk surfaces askError on Work surface.
    }
  }

  async function handleSave(event?: Event) {
    event?.preventDefault();
    await vault.flushSave();
  }

  const saveWhisper = $derived(vault.saveWhisper());
  const showDiffChip = $derived(vault.diffChip());

  function handleWikilink(target: string) {
    vault.openWikilink(target);
  }

  async function handleExportPdf() {
    if (!vault.selectedPath || exportingPdf) return;
    if (vault.dirty) await vault.flushSave();
    exportingPdf = true;
    vault.error = null;
    try {
      await exportVaultNotePdf({
        title: displayTitle,
        content: vault.content,
        labelByPath: vault.labelByPath(),
      });
    } catch (err) {
      vault.error = err instanceof Error ? err.message : String(err);
    } finally {
      exportingPdf = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!vault.selectedPath || mobile) return;

    const tag = (event.target as HTMLElement).tagName;
    const typing = tag === "TEXTAREA" || tag === "INPUT";

    if (typing && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
      event.preventDefault();
      void handleSave();
      return;
    }

    if (typing) return;

    if (event.key === "e" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      if (vault.editorMode === "preview") {
        event.preventDefault();
        vault.enterEditMode();
      }
      return;
    }

    if (event.key === "Escape" && vault.editorMode === "edit" && !typing) {
      if (vault.isWriteFirstKind && !vault.isAuthoringSource) {
        event.preventDefault();
        vault.enterPreviewMode();
        return;
      }
      if (previewFirstKind) {
        event.preventDefault();
        vault.enterPreviewMode();
      }
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });
</script>

<section
  class="vault-editor flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}"
>
  {#if !mobile}
    <header class="vault-editor-header workshop-header flex items-center justify-between gap-3 py-3">
      <div class="min-w-0" title={vault.selectedPath ?? undefined}>
        {#if activeSpace && SpaceIcon}
          <p class="mb-1 flex items-center gap-1.5 text-xs font-medium text-primary-300">
            <SpaceIcon size={13} strokeWidth={2} />
            {activeSpace.label}
          </p>
        {/if}
        {#if breadcrumb}
          <p class="workshop-faint truncate">{breadcrumb}</p>
        {/if}
        <div class="flex min-w-0 items-center gap-2">
          <h1 class="truncate text-base font-semibold">{displayTitle}</h1>
          {#if vault.selectedPath}
            <VaultKindBadge
              kind={vault.selectedKind}
              path={vault.selectedPath}
            />
          {/if}
        </div>
        {#if vault.selectedPath && vault.editorMode === "preview"}
          <p class="mt-1 text-[11px] text-surface-500">
            Press <kbd class="vault-kbd">E</kbd> to edit · type <kbd class="vault-kbd">/</kbd> on a
            new line for blocks
          </p>
        {/if}
      </div>

      <div class="flex shrink-0 flex-wrap items-center justify-end gap-2">
        {#if layout.vaultSidebarCollapsed}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            title="Show library browser"
            aria-label="Show library browser"
            onclick={() => layout.setVaultSidebarCollapsed(false)}
          >
            <PanelLeftOpen size={14} strokeWidth={2} />
          </button>
        {/if}
        {#if vault.selectedPath && !mobile && onOpenChat}
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            disabled={vault.noteLoading}
            onclick={() => void handleAskAboutNote()}
          >
            Ask about note
          </button>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={vault.noteLoading}
            title="Open scoped thread in Chat tab"
            onclick={() => void handleAskInChatTab()}
          >
            Chat tab
          </button>
        {/if}
        {#if vault.selectedPath && !mobile && onOpenWork}
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={vault.noteLoading || workspace.askSubmitting}
            onclick={() => void handleSendToWork()}
          >
            {workspace.askSubmitting ? "Sending…" : "Send to Work"}
          </button>
        {/if}
        {#if linkedWork.length > 0 && onSelectCard}
          {#each linkedWork.slice(0, 2) as card (card.id)}
            <button
              type="button"
              class="badge variant-soft-secondary cursor-pointer text-[10px] font-medium"
              onclick={() => void onSelectCard(card.id)}
            >
              Linked · {formatCardTitle(card)}
            </button>
          {/each}
        {/if}
        {#if vault.selectedKind === "inbox" && vault.selectedPath}
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={vault.saving}
            onclick={() => void vault.promoteNote("journal")}
          >
            → Journal
          </button>
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={vault.saving}
            onclick={() => void vault.promoteNote("projects")}
          >
            → Project
          </button>
        {/if}
        {#if vault.selectedKind === "daily" && vault.editorMode === "edit"}
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            onclick={() => vault.insertWeeklyReviewLink()}
          >
            Link weekly review
          </button>
        {/if}
        {#if vault.selectedKind === "ledger" && vault.editorMode === "edit"}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={() => vault.toggleLedgerEditMode()}
          >
            {vault.ledgerEditMode === "table" ? "Raw markdown" : "Table view"}
          </button>
        {/if}
        {#if hasKanbanBoard && vault.editorMode === "edit"}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={() => vault.toggleBoardEditMode()}
          >
            {vault.boardEditMode === "board" ? "Raw markdown" : "Board view"}
          </button>
        {/if}

        {#if showMarkdownEditor && (!vault.isWriteFirstKind || vault.isAuthoringSource)}
          <button
            type="button"
            class="btn btn-sm {layout.vaultSplitEnabled
              ? 'variant-soft-primary'
              : 'variant-ghost-surface'}"
            onclick={() => layout.toggleVaultSplitEnabled()}
            title="Split edit and preview"
          >
            Split
          </button>
        {/if}

        {#if vault.selectedPath && (vault.wikilinksOut.length > 0 || vault.backlinks.length > 0)}
          <button
            type="button"
            class="btn btn-sm {layout.vaultLinksPanelOpen
              ? 'variant-soft-surface'
              : 'variant-ghost-surface'}"
            onclick={() => layout.toggleVaultLinksPanel()}
          >
            Links
          </button>
        {/if}

        {#if saveWhisper}
          <span
            class="vault-save-whisper text-xs {saveWhisper === 'Saved'
              ? 'text-success-400'
              : 'text-surface-400'}"
          >
            {saveWhisper}
          </span>
        {:else if showDiffChip}
          <span class="badge variant-soft-warning text-xs font-mono">
            {showDiffChip}
          </span>
        {/if}

        {#if vault.dirty && vault.saveStatus !== "conflict"}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={vault.saving}
            onclick={handleSave}
            title="Save now (⌘S)"
          >
            Save now
          </button>
        {/if}

        {#if vault.selectedPath}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            title="Rename, move, or delete note"
            aria-label="Note actions"
            disabled={vault.noteLoading}
            onclick={() => vault.openNoteActions()}
          >
            <MoreHorizontal size={14} strokeWidth={2} />
          </button>
        {/if}
        {#if vault.selectedPath}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={exportingPdf || vault.noteLoading}
            title="Export rendered note as PDF"
            onclick={() => void handleExportPdf()}
          >
            <FileDown size={14} strokeWidth={2} />
            {exportingPdf ? "Exporting…" : "PDF"}
          </button>
        {/if}

        {#if vault.isWriteFirstKind && showMarkdownEditor}
          <button
            type="button"
            class="btn btn-sm {vault.isAuthoringSource
              ? 'variant-soft-primary'
              : 'variant-ghost-surface'}"
            onclick={() => vault.toggleAuthoringMode()}
            title={vault.isAuthoringSource
              ? "Return to write mode"
              : "Show markdown source and split preview"}
          >
            {vault.isAuthoringSource ? "Write" : "Markdown"}
          </button>
        {/if}

        {#if vault.selectedPath}
          <button
            type="button"
            class="btn btn-sm {vault.editorMode === 'preview'
              ? 'variant-soft-primary'
              : 'variant-ghost-surface'}"
            onclick={() =>
              vault.editorMode === "edit"
                ? vault.enterPreviewMode()
                : vault.enterEditMode()}
            title={vault.editorMode === "edit"
              ? "View rendered note"
              : "Return to editing"}
          >
            {vault.editorMode === "edit" ? "Preview" : "Edit"}
          </button>
        {/if}
      </div>
    </header>
  {:else}
    <div class="shrink-0 border-b border-surface-500/40 px-4 py-2">
      {#if breadcrumb}
        <p class="workshop-faint truncate text-xs">{breadcrumb}</p>
      {/if}
      <h1 class="truncate text-sm font-semibold text-surface-50">{displayTitle}</h1>
    </div>
  {/if}

  {#if vault.error}
    <p class="border-b border-error-500/30 bg-error-500/10 px-4 py-2 text-xs text-error-300">
      {vault.error}
    </p>
  {/if}

  <VaultProposalBar {mobile} />
  <VaultConflictBar />
  <VaultAttachmentBar disabled={vault.noteLoading || vault.saving} />
  <VaultAttachmentPreview />

  {#if !vault.selectedPath}
    <VaultEmptyState />
  {:else if vault.noteLoading}
    <div class="flex flex-1 items-center justify-center text-sm text-surface-400">
      Loading note…
    </div>
  {:else}
    <div class="flex min-h-0 flex-1">
      <div class="flex min-h-0 min-w-0 flex-1 flex-col">
        {#if showLedgerTable}
          <LedgerTableEditor
            content={vault.content}
            disabled={vault.saving}
            onchange={(next) => vault.markDirty(next)}
          />
        {:else if showKanbanBoard}
          <KanbanBoardEditor
            content={vault.content}
            disabled={vault.saving}
            onchange={(next) => vault.markDirty(next)}
            onWikilink={handleWikilink}
          />
        {:else if showMarkdownEditor}
          <VaultMarkdownEditor
            content={vault.content}
            contentSyncKey={vault.contentSyncKey}
            disabled={vault.noteLoading}
            class="flex-1"
            surface={editorSurface}
            split={showSplitEditor}
            splitWidth={layout.vaultEditorPaneWidth}
            onSplitResize={(width) => layout.setVaultEditorPaneWidth(width)}
            onchange={(next) => vault.markDirty(next)}
          >
            {#snippet preview()}
              <VaultMarkdownPreview
                content={vault.content}
                {labelByPath}
                compact
                onWikilink={handleWikilink}
              />
            {/snippet}
          </VaultMarkdownEditor>
        {:else if showPreviewOnly}
          <VaultMarkdownPreview
            content={vault.content}
            {labelByPath}
            onWikilink={handleWikilink}
          />
        {/if}
      </div>

      {#if showLinksPanel}
        <VaultNoteLinksPanel
          wikilinksOut={vault.wikilinksOut}
          backlinks={vault.backlinks}
          {labelByPath}
          onOpenNote={(path) => vault.openNote(path)}
        />
      {/if}
    </div>
  {/if}
</section>

<VaultNoteActionsMenu />
