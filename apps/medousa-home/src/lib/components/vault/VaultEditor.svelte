<script lang="ts">
  import { onMount, tick } from "svelte";
  import { PanelLeftOpen, Search } from "@lucide/svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { noteWorkshop } from "$lib/stores/noteWorkshop.svelte";
  import { vaultBreadcrumb, vaultDisplayTitle } from "$lib/utils/formatVault";
  import {
    buildWorkAskFromNote,
    prepareTalkAboutNote,
  } from "$lib/utils/vaultNoteBridge";
  import { launchVaultNoteWorkshop } from "$lib/utils/vaultNoteWorkshop";
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
  import VaultEditorOverflowMenu from "./VaultEditorOverflowMenu.svelte";
  import VaultLinkedFilesMenu from "./VaultLinkedFilesMenu.svelte";
  import {
    supportsLinksPanel,
    supportsPreviewSplit,
  } from "$lib/utils/vaultNoteKind";
  import VaultNoteChatFab from "./VaultNoteChatFab.svelte";
  import VaultFindBar from "./VaultFindBar.svelte";
  import VaultNoteStatusBar from "./VaultNoteStatusBar.svelte";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import { vaultQuickSwitcher } from "$lib/stores/vaultQuickSwitcher.svelte";
  import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
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
  let lastFindNotePath = $state<string | null>(null);

  const displayTitle = $derived(
    vault.selectedPath
      ? (vault.labelByPathMap.get(vault.selectedPath) ??
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

  const showBreadcrumb = $derived.by(() => {
    if (!breadcrumb) return false;
    if (activeSpace && breadcrumb.toLowerCase() === activeSpace.label.toLowerCase()) {
      return false;
    }
    return true;
  });

  const labelByPath = $derived(vault.labelByPathMap);
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
    !mobile && showMarkdownEditor && layout.vaultSplitEnabled,
  );

  const editorSurface = $derived<"source">("source");

  const showPreviewOnly = $derived(
    vault.editorMode === "preview" ||
      (!showMarkdownEditor && !showLedgerTable),
  );

  const noteKind = $derived(vault.selectedKind);
  const linkCount = $derived(vault.wikilinksOut.length + vault.backlinks.length);
  const showLinksToggle = $derived(
    Boolean(vault.selectedPath) && supportsLinksPanel(noteKind) && linkCount > 0,
  );
  const showLinksPanel = $derived(
    !mobile && layout.vaultLinksPanelOpen && showLinksToggle,
  );
  const showPreviewButton = $derived(
    Boolean(vault.selectedPath) && supportsPreviewSplit(noteKind),
  );
  const showSplitButton = $derived(
    showMarkdownEditor && supportsPreviewSplit(noteKind),
  );
  const showLedgerViewToggle = $derived(
    Boolean(vault.selectedPath) &&
      vault.selectedKind === "ledger" &&
      vault.editorMode === "edit" &&
      hasLedgerTable,
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

  const findSupported = $derived(
    Boolean(vault.selectedPath) &&
      !vault.noteLoading &&
      (showMarkdownEditor ||
        (showPreviewOnly && !showLedgerTable && !showKanbanBoard)),
  );

  const findSourceText = $derived(
    showMarkdownEditor && vault.editorMode === "edit"
      ? vault.content
      : stripFrontmatter(vault.content).content,
  );

  const findMode = $derived<"edit" | "preview">(
    showMarkdownEditor && vault.editorMode === "edit" ? "edit" : "preview",
  );

  const showNoteStatus = $derived(
    Boolean(vault.selectedPath) &&
      !vault.noteLoading &&
      !showLedgerTable &&
      !showKanbanBoard,
  );

  $effect(() => {
    const path = vault.selectedPath;
    if (path === lastFindNotePath) return;
    lastFindNotePath = path;
    vaultFind.reset();
  });

  $effect(() => {
    if (vault.selectedPath && !mobile) {
      void workspace.prefetchVaultLinkedWork(vault.selectedPath);
    }
  });

  function syncFind() {
    if (!vaultFind.open || !findSupported) return;
    vaultFind.syncAndReveal(findMode);
  }

  $effect(() => {
    if (!vaultFind.open || !findSupported) return;
    vaultFind.query;
    vaultFind.revealEpoch;
    vaultFind.matchIndex;
    vaultFind.sourceText;
    findMode;
    void tick().then(() => syncFind());
  });

  async function handleAskInChatTab() {
    if (!vault.selectedPath) return;

    // Mobile: no floating workshop yet — jump to the chat tab with note context.
    if (mobile) {
      if (!onOpenChat) return;
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
      return;
    }

    // Desktop: stay in the note with the floating IM-style workshop.
    await launchVaultNoteWorkshop({
      path: vault.selectedPath,
      title: vault.title,
      content: vault.content,
      wikilinksOut: vault.wikilinksOut,
      backlinks: vault.backlinks,
      session: "fresh",
      flushSave: vault.dirty
        ? async () => {
            await vault.flushSave();
          }
        : undefined,
    });
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
  const showDiffChip = $derived(vault.diffChipText);

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
        labelByPath: vault.labelByPathMap,
      });
    } catch (err) {
      vault.error = err instanceof Error ? err.message : String(err);
    } finally {
      exportingPdf = false;
    }
  }

  function handleFindShortcut(event: KeyboardEvent) {
    if (!vault.selectedPath || !findSupported) return;
    if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "f") return;
    event.preventDefault();
    event.stopPropagation();
    vaultFind.setSourceText(findSourceText);
    vaultFind.openFind();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!vault.selectedPath) return;

    if (vaultFind.open && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      vaultFind.close();
      return;
    }

    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
      handleFindShortcut(event);
      return;
    }

    if (mobile) return;

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

    if (event.key === "Escape" && vault.editorMode === "edit" && !typing && !vaultFind.open) {
      if (previewFirstKind) {
        event.preventDefault();
        vault.enterPreviewMode();
      }
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown, true);
    return () => window.removeEventListener("keydown", handleKeydown, true);
  });
</script>

<section
  class="vault-editor relative flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}"
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
        {#if showBreadcrumb}
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
            Press <kbd class="vault-kbd">E</kbd> to edit · <kbd class="vault-kbd">⌘F</kbd> to find
            · type <kbd class="vault-kbd">/</kbd> on a new line for blocks
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

        {#if showPreviewButton}
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

        {#if showSplitButton}
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

        {#if showLinksToggle}
          <button
            type="button"
            class="btn btn-sm {layout.vaultLinksPanelOpen
              ? 'variant-soft-surface'
              : 'variant-ghost-surface'}"
            onclick={() => layout.toggleVaultLinksPanel()}
            title="Show note links"
          >
            Links
            <span class="tabular-nums text-surface-400">({linkCount})</span>
          </button>
        {/if}

        {#if showLedgerViewToggle}
          <div class="ledger-mode-toggle" role="group" aria-label="Ledger view">
            <button
              type="button"
              class="ledger-mode-btn {vault.ledgerEditMode === 'table'
                ? 'ledger-mode-btn-active'
                : ''}"
              onclick={() => vault.setLedgerEditMode("table")}
            >
              Table
            </button>
            <button
              type="button"
              class="ledger-mode-btn {vault.ledgerEditMode === 'raw'
                ? 'ledger-mode-btn-active'
                : ''}"
              onclick={() => vault.setLedgerEditMode("raw")}
            >
              Raw
            </button>
          </div>
        {/if}

        {#if vault.selectedPath}
          <VaultLinkedFilesMenu disabled={vault.noteLoading || vault.saving} />
        {/if}

        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          title="Find note (⌘O)"
          aria-label="Find note"
          onclick={() => vaultQuickSwitcher.openSwitcher()}
        >
          <Search size={14} strokeWidth={2} />
        </button>

        <VaultEditorOverflowMenu
          selectedPath={vault.selectedPath}
          selectedKind={vault.selectedKind}
          editorMode={vault.editorMode}
          noteLoading={vault.noteLoading}
          saving={vault.saving}
          dirty={vault.dirty}
          saveStatus={vault.saveStatus}
          exportingPdf={exportingPdf}
          askSubmitting={workspace.askSubmitting}
          hasKanbanBoard={hasKanbanBoard}
          boardEditMode={vault.boardEditMode}
          linkedWork={linkedWork}
          onOpenChat={onOpenChat}
          onOpenWork={onOpenWork}
          onSelectCard={onSelectCard}
          onExportPdf={handleExportPdf}
          onAskInChat={handleAskInChatTab}
          onSendToWork={handleSendToWork}
          onSave={handleSave}
          onOpenNoteActions={() => vault.openNoteActions()}
          onInsertWeeklyReview={() => vault.insertWeeklyReviewLink()}
          onPromoteJournal={() => vault.promoteNote("journal")}
          onPromoteProject={() => vault.promoteNote("projects")}
          onToggleBoard={() => vault.toggleBoardEditMode()}
        />
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

  {#if showLinksToggle && !layout.vaultLinksPanelOpen && !mobile}
    <div class="flex shrink-0 items-center gap-2 border-b border-surface-500/30 px-4 py-1.5 text-xs">
      <span class="text-surface-500">{linkCount} linked note{linkCount === 1 ? "" : "s"}</span>
      <button
        type="button"
        class="text-primary-300 hover:text-primary-200"
        onclick={() => layout.setVaultLinksPanelOpen(true)}
      >
        Show links
      </button>
    </div>
  {/if}

  {#if !vault.selectedPath}
    <VaultEmptyState />
  {:else if vault.noteLoading}
    <div class="flex flex-1 items-center justify-center text-sm text-surface-400">
      Loading note…
    </div>
  {:else}
    <div class="flex min-h-0 flex-1">
      <div class="relative flex min-h-0 min-w-0 flex-1 flex-col">
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
        {#if findSupported && vaultFind.open}
          <VaultFindBar />
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

  {#if showNoteStatus}
    <VaultNoteStatusBar
      content={vault.content}
      tags={vault.noteTags}
      editorMode={vault.editorMode}
    />
  {/if}

  {#if vault.selectedPath && !mobile && !noteWorkshop.open}
    <VaultNoteChatFab />
  {/if}
</section>

<VaultNoteActionsMenu />
