<script lang="ts">
  import "$lib/styles/vault-editor.postcss";
  import { tick } from "svelte";
  import {
    BookOpen,
    Code2,
    Columns3,
    Search,
    Table2,
  } from "@lucide/svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { noteWorkshop } from "$lib/stores/noteWorkshop.svelte";
  import { vaultBreadcrumb, vaultDisplayTitle } from "$lib/utils/formatVault";
  import {
    sendVaultNoteToWork,
    talkAboutVaultNote,
  } from "$lib/utils/vaultNoteWorkshop";
  import { findLedgerTable } from "$lib/utils/markdownTable";
  import { findKanbanBoard, noteHasKanbanBoard } from "$lib/utils/markdownKanban";
  import { noteHasSlidesDeck } from "$lib/utils/markdownSlides";
  import { noteHasDraw } from "$lib/draw/drawDocument";
  import { kindFromNoteContent } from "$lib/utils/dataFirstSurface";
  import { parseWorkbookManifest } from "$lib/utils/workbook";
  import VaultEmptyState from "./VaultEmptyState.svelte";
  import VaultKindBadge from "./VaultKindBadge.svelte";
  import LedgerTableEditor from "./LedgerTableEditor.svelte";
  import KanbanBoardEditor from "./KanbanBoardEditor.svelte";
  import SlidesDeckEditor from "./SlidesDeckEditor.svelte";
  import DrawNoteEditor from "./DrawNoteEditor.svelte";
  import WorkbookManifestEditor from "./WorkbookManifestEditor.svelte";
  import VaultMarkdownPreview from "./VaultMarkdownPreview.svelte";
  import VaultNoteLinksPanel from "./VaultNoteLinksPanel.svelte";
  import VaultConflictBar from "./VaultConflictBar.svelte";
  import VaultProposalBar from "./VaultProposalBar.svelte";
  import VaultVersionsPanel from "./VaultVersionsPanel.svelte";
  import VaultMarkdownEditor from "./VaultMarkdownEditor.svelte";
  import VaultNoteActionsMenu from "./VaultNoteActionsMenu.svelte";
  import VaultViewBuilderSheet from "./VaultViewBuilderSheet.svelte";
  import VaultEditorOverflowMenu from "./VaultEditorOverflowMenu.svelte";
  import VaultLinkedFilesMenu from "./VaultLinkedFilesMenu.svelte";
  import VaultPathBar from "./VaultPathBar.svelte";
  import {
    supportsLinksPanel,
    supportsPreviewSplit,
  } from "$lib/utils/vaultNoteKind";
  import VaultNoteChatFab from "./VaultNoteChatFab.svelte";
  import VaultFindBar from "./VaultFindBar.svelte";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import { noteEditorRuntimes } from "$lib/vault/noteEditorRuntimes.svelte";
  import { registerVaultLeaveFlush } from "$lib/vault/vaultLeaveFlush";
  import { vaultQuickSwitcher } from "$lib/stores/commandSpotlight.svelte";
  import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
  import { titleWithShortcut } from "$lib/utils/keyboardShortcutsCatalog";
  import { writeVaultStickyPath } from "$lib/utils/vaultSticky";
  import {
    isPlainTextEditingTarget,
    matchVaultHotkey,
  } from "$lib/utils/vaultHotkeys";
  import { isTauri, showVaultSticky } from "$lib/window";
  import type { VaultExportFormat } from "$lib/utils/vaultExportOptions";
  import VaultExportPreviewModal from "./VaultExportPreviewModal.svelte";
  import VaultChartBuilderSheet from "./VaultChartBuilderSheet.svelte";
  import VaultLiquidBuilderSheet from "./VaultLiquidBuilderSheet.svelte";
  import LiquidCardDetailSheet from "$lib/components/chat/LiquidCardDetailSheet.svelte";
  import MarkdownHeadingOutline from "$lib/components/ui/MarkdownHeadingOutline.svelte";
  import {
    extractMarkdownHeadings,
    observeActiveMarkdownHeading,
    scrollElementWithinContainer,
    scrollToHeadingInContainer,
  } from "$lib/utils/headingSlug";

  interface Props {
    visible: boolean;
    /** Focused shell pane — hotkeys + editing. Background tiled panes stay read-only. */
    interactive?: boolean;
    /** Bound note path for multi-pane Workspace (background panes keep their own buffer). */
    path?: string | null;
    /**
     * Keep TipTap/CM mounted while this host is not focused (hidden keep-alive pool).
     * Unfocused keep-alive hosts stay inert and do not fall back to preview-only.
     */
    keepAlive?: boolean;
    /** Mobile reader: preview-only, no edit chrome. */
    mobile?: boolean;
    /** Sticky pop-out: slim companion chrome, keep note IM chat. */
    stickyNote?: boolean;
    onOpenChat?: () => void;
    onOpenWork?: () => void;
    onSelectCard?: (id: string) => void | Promise<void>;
  }

  let {
    visible,
    interactive = true,
    path = null,
    keepAlive = false,
    mobile = false,
    stickyNote = false,
    onOpenChat,
    onOpenWork,
    onSelectCard,
  }: Props = $props();

  const notePath = $derived(path?.trim() || vault.selectedPath);
  const bound = $derived(!path?.trim() || vault.isFocusedPath(path));
  /** Focused editor, or keep-alive host that must retain TipTap/CM. */
  const liveHost = $derived(bound || keepAlive);
  const displayContent = $derived(notePath ? vault.contentFor(notePath) : "");
  const displaySyncKey = $derived(notePath ? vault.contentSyncKeyFor(notePath) : "");
  const displayLoading = $derived(notePath ? vault.noteLoadingFor(notePath) : false);

  let exportingPdf = $state(false);
  let exportingWord = $state(false);
  let exportPreviewOpen = $state(false);
  let exportPreviewFormat = $state<VaultExportFormat>("pdf");
  let exportPreviewTitle = $state("");
  let exportPreviewContent = $state("");
  let exportPreviewLabels = $state<Map<string, string>>(new Map());
  let exportPreviewPath = $state<string | null>(null);
  let lastFindNotePath = $state<string | null>(null);
  const VAULT_OUTLINE_MODE_KEY = "medousa.vault.outlineMode";

  function readVaultOutlineMode(): "panel" | "rail" {
    if (typeof sessionStorage === "undefined") return "rail";
    try {
      return sessionStorage.getItem(VAULT_OUTLINE_MODE_KEY) === "panel"
        ? "panel"
        : "rail";
    } catch {
      return "rail";
    }
  }

  let previewScrollEl = $state<HTMLElement | null>(null);
  let markdownEditorEl = $state<ReturnType<typeof VaultMarkdownEditor> | null>(null);
  let slidesDeckEl = $state<ReturnType<typeof SlidesDeckEditor> | null>(null);
  let outlineActiveId = $state<string | null>(null);
  let outlineScrolling = $state(false);
  let outlineMode = $state<"panel" | "rail">(readVaultOutlineMode());

  function setOutlineMode(mode: "panel" | "rail") {
    outlineMode = mode;
    try {
      sessionStorage.setItem(VAULT_OUTLINE_MODE_KEY, mode);
    } catch {
      // ignore quota / private mode
    }
  }

  function toggleOutlineMode() {
    setOutlineMode(outlineMode === "rail" ? "panel" : "rail");
  }

  const displayTitle = $derived(
    bound && vault.isLooseFile && vault.looseFilePath
      ? (vault.looseFilePath.split(/[/\\]/).pop() ?? vault.looseFilePath)
      : notePath
        ? // Prefer live draft title so Properties edits update chrome immediately.
          vaultDisplayTitle(
            bound ? vault.title : vault.titleFor(notePath),
            notePath,
          ) ||
          vault.labelByPathMap.get(notePath) ||
          "Untitled"
        : "Library",
  );

  const breadcrumb = $derived(
    bound && vault.isLooseFile && vault.looseFilePath
      ? vault.looseFilePath
      : notePath
        ? vaultBreadcrumb(notePath)
        : null,
  );

  const labelByPath = $derived(vault.labelByPathMap);
  /** Content frontmatter wins so chrome kind can't snap sheet → note after buffer restore. */
  const contentKind = $derived(kindFromNoteContent(notePath, displayContent));
  const sheetLikeKind = $derived(
    contentKind === "ledger" ||
      contentKind === "sheet" ||
      vault.selectedKind === "ledger" ||
      vault.selectedKind === "sheet",
  );
  const workbookKind = $derived(
    contentKind === "workbook" || vault.selectedKind === "workbook",
  );
  const hasLedgerTable = $derived(Boolean(findLedgerTable(displayContent)));
  const hasWorkbookManifest = $derived(
    workbookKind && Boolean(parseWorkbookManifest(displayContent)),
  );
  const hasKanbanBoard = $derived(noteHasKanbanBoard(displayContent));
  const kanbanBoard = $derived(hasKanbanBoard ? findKanbanBoard(displayContent) : null);
  const hasSlidesDeck = $derived(noteHasSlidesDeck(displayContent));
  const drawKind = $derived(
    contentKind === "draw" || vault.selectedKind === "draw",
  );
  const hasDraw = $derived(noteHasDraw(displayContent));

  const showLedgerTable = $derived(
    !mobile &&
      vault.editorMode === "edit" &&
      sheetLikeKind &&
      vault.ledgerEditMode === "table" &&
      hasLedgerTable,
  );

  const showWorkbookManifest = $derived(
    !mobile &&
      vault.editorMode === "edit" &&
      workbookKind &&
      vault.workbookEditMode === "view" &&
      hasWorkbookManifest,
  );

  const showKanbanBoard = $derived(
    hasKanbanBoard &&
      kanbanBoard !== null &&
      (vault.editorMode === "preview" ||
        (vault.editorMode === "edit" && vault.boardEditMode === "board")),
  );

  const showSlidesDeck = $derived(
    hasSlidesDeck &&
      (vault.editorMode === "preview" ||
        (vault.editorMode === "edit" && vault.deckEditMode === "deck")),
  );

  const showDrawSurface = $derived(drawKind && hasDraw);

  const showMarkdownEditor = $derived(
    // Keep-alive hosts retain TipTap even while unfocused (global mode is for the focused note).
    (keepAlive && !bound) ||
      (vault.editorMode === "edit" &&
        !showLedgerTable &&
        !showWorkbookManifest &&
        !showKanbanBoard &&
        !showSlidesDeck &&
        !showDrawSurface),
  );

  const notePlane = $derived(vault.notePlane);
  const isLivePlane = $derived(notePlane === "live");
  const isBuildPlane = $derived(notePlane === "build");

  const showSplitEditor = $derived(
    !mobile &&
      !stickyNote &&
      showMarkdownEditor &&
      isBuildPlane &&
      layout.vaultSplitEnabled,
  );

  /** Live always uses write typography; Build respects source/write choice. */
  const editorSurface = $derived<"write" | "source">(
    isLivePlane ? "write" : vault.editorSurface,
  );

  const showPreviewOnly = $derived(
    vault.editorMode === "preview" &&
      !showKanbanBoard &&
      !showLedgerTable &&
      !showWorkbookManifest &&
      !showSlidesDeck &&
      !showDrawSurface,
  );

  /** Preview surface is mounted (preview-only or Build split). */
  const showPreviewSurface = $derived(
    Boolean(notePath) && liveHost && (showPreviewOnly || showSplitEditor),
  );

  /** Live TipTap note surface (outline jumps against stamped headings). */
  const showLiveOutlineSurface = $derived(
    Boolean(notePath) &&
      bound &&
      liveHost &&
      showMarkdownEditor &&
      isLivePlane &&
      !mobile &&
      !stickyNote,
  );

  const showOutlineSurface = $derived(showPreviewSurface || showLiveOutlineSurface);

  const outlineItems = $derived.by(() => {
    if (!showOutlineSurface) return [];
    const body = stripFrontmatter(displayContent).content;
    return extractMarkdownHeadings(body)
      .filter((h) => h.depth <= 3)
      .map((h) => ({ id: h.slug, text: h.text, depth: h.depth }));
  });

  const showHeadingOutline = $derived(
    showOutlineSurface && outlineItems.length > 0 && !mobile && !stickyNote,
  );

  function resolveOutlineScrollEl(): HTMLElement | null {
    if (showPreviewSurface) return previewScrollEl;
    if (showLiveOutlineSurface) return markdownEditorEl?.getScrollEl?.() ?? null;
    return null;
  }

  function jumpToOutlineHeading(id: string) {
    const scrollRoot = resolveOutlineScrollEl();
    if (!scrollRoot) return;
    if (scrollToHeadingInContainer(scrollRoot, id)) return;
    const item = outlineItems.find((entry) => entry.id === id);
    if (item && scrollToHeadingInContainer(scrollRoot, item.text)) return;

    // Last resort: same ordinal among visible h1–h3 (slug mismatches in Live).
    const index = outlineItems.findIndex((entry) => entry.id === id);
    if (index < 0) return;
    const headings = scrollRoot.querySelectorAll<HTMLElement>(
      "h1.markdown-heading, h2.markdown-heading, h3.markdown-heading, h1, h2, h3",
    );
    const unique: HTMLElement[] = [];
    const seen = new Set<HTMLElement>();
    for (const heading of headings) {
      if (seen.has(heading)) continue;
      seen.add(heading);
      unique.push(heading);
    }
    const target = unique[index];
    if (target) scrollElementWithinContainer(scrollRoot, target);
  }

  $effect(() => {
    if (!showHeadingOutline) {
      outlineActiveId = null;
      outlineScrolling = false;
      return;
    }
    // Bind once per surface/host — do NOT rebind on every draft keystroke.
    // MutationObserver (debounced) picks up heading DOM changes.
    const previewHost = showPreviewSurface;
    const liveHostOutline = showLiveOutlineSurface;
    const previewEl = previewScrollEl;
    const editor = markdownEditorEl;
    notePath;

    let cancelled = false;
    let cleanup: (() => void) | undefined;
    let attempts = 0;

    const bind = () => {
      if (cancelled) return;
      const scrollRoot = previewHost
        ? previewEl
        : liveHostOutline
          ? (editor?.getScrollEl?.() ?? null)
          : null;
      if (!scrollRoot) {
        if (attempts++ < 12) {
          void tick().then(bind);
        } else {
          outlineActiveId = null;
        }
        return;
      }
      const contentRoot = liveHostOutline
        ? (scrollRoot.querySelector<HTMLElement>(".vault-live-prose") ??
          scrollRoot)
        : scrollRoot;
      cleanup?.();
      const stopObservingActiveHeading = observeActiveMarkdownHeading(
        scrollRoot,
        (id) => {
          outlineActiveId = id;
        },
        contentRoot,
      );
      let scrollEndTimer: ReturnType<typeof setTimeout> | undefined;
      const onOutlineScroll = () => {
        outlineScrolling = true;
        if (scrollEndTimer) clearTimeout(scrollEndTimer);
        scrollEndTimer = setTimeout(() => {
          outlineScrolling = false;
          scrollEndTimer = undefined;
        }, 160);
      };
      scrollRoot.addEventListener("scroll", onOutlineScroll, { passive: true });
      cleanup = () => {
        stopObservingActiveHeading();
        scrollRoot.removeEventListener("scroll", onOutlineScroll);
        if (scrollEndTimer) clearTimeout(scrollEndTimer);
        outlineScrolling = false;
      };
    };

    void tick().then(bind);
    return () => {
      cancelled = true;
      cleanup?.();
    };
  });

  const noteKind = $derived(
    contentKind !== "note" ? contentKind : vault.selectedKind,
  );
  const linkCount = $derived(vault.wikilinksOut.length + vault.backlinks.length);
  const showLinksToggle = $derived(
    isBuildPlane &&
      !stickyNote &&
      !vault.isLooseFile &&
      Boolean(vault.selectedPath) &&
      supportsLinksPanel(noteKind) &&
      linkCount > 0,
  );
  const showLinksPanel = $derived(
    !mobile &&
      !stickyNote &&
      isBuildPlane &&
      layout.vaultLinksPanelOpen &&
      showLinksToggle,
  );
  const showPreviewButton = $derived(
    Boolean(vault.selectedPath) && supportsPreviewSplit(noteKind),
  );
  const showSplitButton = $derived(
    isBuildPlane &&
      !stickyNote &&
      showMarkdownEditor &&
      supportsPreviewSplit(noteKind),
  );
  /** Live | Build pill for markdown note editing (not ledger/board surfaces). */
  const showNotePlaneToggle = $derived(
    !mobile &&
      !stickyNote &&
      Boolean(vault.selectedPath) &&
      supportsPreviewSplit(noteKind) &&
      !showLedgerTable &&
      !showWorkbookManifest &&
      !showKanbanBoard &&
      !showSlidesDeck &&
      !showDrawSurface,
  );

  $effect(() => {
    if (isLivePlane && layout.vaultLinksPanelOpen) {
      layout.setVaultLinksPanelOpen(false);
    }
  });

  const showLedgerViewToggle = $derived(
    Boolean(vault.selectedPath) &&
      sheetLikeKind &&
      vault.editorMode === "edit" &&
      hasLedgerTable,
  );

  const showWorkbookViewToggle = $derived(
    Boolean(vault.selectedPath) &&
      workbookKind &&
      vault.editorMode === "edit" &&
      hasWorkbookManifest,
  );

  const showBoardViewToggle = $derived(
    Boolean(vault.selectedPath) &&
      hasKanbanBoard &&
      vault.editorMode === "edit",
  );

  const showDeckViewToggle = $derived(
    Boolean(vault.selectedPath) &&
      hasSlidesDeck &&
      vault.editorMode === "edit",
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
        (showPreviewOnly &&
          !showLedgerTable &&
          !showWorkbookManifest &&
          !showKanbanBoard &&
          !showSlidesDeck &&
          !showDrawSurface)),
  );

  const findSourceText = $derived(
    showMarkdownEditor && vault.editorMode === "edit"
      ? displayContent
      : stripFrontmatter(displayContent).content,
  );

  const findMode = $derived<"edit" | "preview">(
    showMarkdownEditor && vault.editorMode === "edit" ? "edit" : "preview",
  );

  $effect(() => {
    if (!bound || !interactive) return;
    const path = vault.selectedPath;
    if (!path || path === lastFindNotePath) return;
    // Stash find for the note we are leaving; restore per-path find from runtime.
    if (lastFindNotePath) {
      noteEditorRuntimes.patchUi(lastFindNotePath, {
        find: {
          query: vaultFind.query,
          matchIndex: vaultFind.matchIndex,
          matchCase: vaultFind.matchCase,
        },
      });
    }
    lastFindNotePath = path;
    const runtime = noteEditorRuntimes.ensure(path);
    const find = runtime.ui.find;
    if (find?.query) {
      vaultFind.query = find.query;
      vaultFind.matchCase = find.matchCase;
      vaultFind.matchIndex = find.matchIndex;
    } else {
      vaultFind.reset();
    }
  });

  $effect(() => {
    if (!bound || !interactive || !visible) return;
    registerVaultLeaveFlush(() => {
      flushPendingEditorDrafts();
      const scrollTop = markdownEditorEl?.getScrollTop?.() ?? 0;
      vault.stashEditorScroll(vault.selectedPath, scrollTop);
    });
    return () => registerVaultLeaveFlush(null);
  });

  /** Apply stashed scroll after openNote / pane focus restores UI. */
  $effect(() => {
    const epoch = vault.editorScrollRestoreEpoch;
    const path = vault.editorScrollRestorePath;
    const top = vault.editorScrollRestoreTop;
    if (!bound || !notePath || !path || path !== notePath) return;
    if (epoch <= 0 || top <= 0) return;
    void tick().then(() => {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          if (vault.editorScrollRestoreEpoch !== epoch) return;
          markdownEditorEl?.setScrollTop?.(top);
        });
      });
    });
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
    vaultFind.matchCase;
    findMode;
    void tick().then(() => syncFind());
  });

  async function handleAskInChatTab() {
    if (!vault.selectedPath) return;
    if (mobile && !onOpenChat) return;
    await talkAboutVaultNote(vault.selectedPath);
    if (mobile) onOpenChat?.();
  }

  async function handleSendToWork() {
    if (!vault.selectedPath || !onOpenWork) return;
    try {
      await sendVaultNoteToWork(vault.selectedPath);
      onOpenWork();
    } catch {
      // workspace.submitAsk surfaces askError on Work surface.
    }
  }

  /** Promote nested Write drafts (Live slides/report, deck editor) before disk save. */
  function flushPendingEditorDrafts() {
    slidesDeckEl?.flush();
    const flushed = markdownEditorEl?.flushLive();
    if (typeof flushed === "string" && flushed !== vault.content) {
      vault.markDirty(flushed, { path: notePath });
    }
  }

  async function handleSave(event?: Event) {
    event?.preventDefault();
    flushPendingEditorDrafts();
    await vault.flushSave();
  }

  const canFloatSticky = $derived(
    !stickyNote &&
      isTauri() &&
      Boolean(vault.selectedPath) &&
      !vault.isLooseFile,
  );

  async function handleFloatSticky() {
    if (!vault.selectedPath || !canFloatSticky) return;
    if (vault.dirty) await vault.flushSave();
    writeVaultStickyPath(vault.selectedPath);
    await showVaultSticky();
  }

  const saveWhisper = $derived(vault.saveWhisper());
  const showDiffChip = $derived(vault.diffChipText);

  function handleWikilink(target: string) {
    vault.openWikilink(target);
  }

  async function openExportPreview(format: VaultExportFormat) {
    if (!vault.selectedPath || exportPreviewOpen) return;
    if (vault.dirty) await vault.flushSave();
    vault.error = null;
    exportPreviewFormat = format;
    exportPreviewTitle = displayTitle;
    exportPreviewContent = vault.content;
    exportPreviewLabels = vault.labelByPathMap;
    exportPreviewPath = vault.selectedPath;
    exportPreviewOpen = true;
  }

  async function handleExportPdf() {
    await openExportPreview("pdf");
  }

  async function handleExportWord() {
    await openExportPreview("docx");
  }

  function handleExportPreviewClose() {
    exportPreviewOpen = false;
    exportingPdf = false;
    exportingWord = false;
  }

  function handleFindShortcut(event: KeyboardEvent) {
    if (!vault.selectedPath || !findSupported) return;
    if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "f") return;
    event.preventDefault();
    event.stopPropagation();
    vaultFind.setSourceText(findSourceText);
    vaultFind.openFind();
  }

  async function openNewNoteFromHotkey() {
    if (vault.newNoteDialogOpen) return;
    if (vault.dirty) await vault.flushSave();
    vault.openNewNoteDialog();
  }

  function handleKeydown(event: KeyboardEvent) {
    const action = matchVaultHotkey(event);

    // New note works even with no note selected (empty library / flow-state).
    if (action === "newNote") {
      if (mobile || vault.newNoteDialogOpen) return;
      event.preventDefault();
      event.stopPropagation();
      void openNewNoteFromHotkey();
      return;
    }

    if (!vault.selectedPath) return;

    if (vaultFind.open && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      vaultFind.close();
      return;
    }

    if (!action) return;

    const typing = isPlainTextEditingTarget(event.target);
    const modChord = event.metaKey || event.ctrlKey;

    // Mod chords work from editors; bare keys do not while typing.
    if (typing && !modChord) return;
    if (mobile && action !== "save" && action !== "find") return;

    if (action === "find") {
      handleFindShortcut(event);
      return;
    }

    if (action === "save") {
      event.preventDefault();
      void handleSave();
      return;
    }

    if (action === "togglePlane") {
      if (!showNotePlaneToggle) return;
      event.preventDefault();
      if (isLivePlane) {
        flushPendingEditorDrafts();
        vault.setNotePlane("build");
      } else {
        vault.setNotePlane("live");
      }
      return;
    }

    if (action === "exportPdf") {
      if (vault.isLooseFile) return;
      event.preventDefault();
      void handleExportPdf();
      return;
    }

    if (action === "toggleBoard") {
      if (!hasKanbanBoard || vault.isLooseFile) return;
      event.preventDefault();
      vault.toggleBoardEditMode();
      return;
    }

    if (action === "enterEdit") {
      if (vault.editorMode === "preview") {
        event.preventDefault();
        vault.enterEditMode();
      }
      return;
    }

    if (action === "enterPreview") {
      if (vault.editorMode === "edit" && !typing && !vaultFind.open && previewFirstKind) {
        event.preventDefault();
        vault.enterPreviewMode();
      }
    }
  }

  $effect(() => {
    if (!interactive || !visible) return;
    window.addEventListener("keydown", handleKeydown, true);
    return () => window.removeEventListener("keydown", handleKeydown, true);
  });
</script>

<section
  class="vault-editor relative flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}"
  data-reading-palette={vault.readingPalette}
  data-note-kind={noteKind}
>
  {#if !mobile && !stickyNote}
    <header
      class="vault-editor-header workshop-header flex items-center gap-2 px-3 py-1"
      title={notePath ? `${displayTitle} — ${notePath}` : displayTitle}
    >
      {#if bound && vault.isLooseFile}
        <span
          class="badge variant-soft-warning shrink-0 text-xs font-medium"
          title="Editing a single file outside the vault"
        >
          Loose file
        </span>
      {:else if bound && notePath}
        <VaultPathBar path={notePath} title={displayTitle} />
      {/if}

      <div class="vault-editor-tools ml-auto flex shrink-0 items-center justify-end gap-0.5">
        {#if bound}
        <ShellSidebarExpandButton label="Show workspace browser" />

        {#if saveWhisper}
          <span
            class="vault-save-whisper text-xs {saveWhisper === 'Saved'
              ? 'text-content-success'
              : 'text-content-tertiary'}"
          >
            {saveWhisper}
          </span>
        {:else if showDiffChip}
          <span class="badge variant-soft-warning text-xs font-mono">
            {showDiffChip}
          </span>
        {/if}

        {#if showNotePlaneToggle && isBuildPlane}
          <button
            type="button"
            class="vault-editor-icon-btn"
            title="Back to Live"
            aria-label="Back to Live"
            onclick={() => vault.setNotePlane("live")}
          >
            <BookOpen size={15} strokeWidth={1.75} />
          </button>
        {/if}

        {#if showLedgerViewToggle}
          <div class="vault-editor-icon-pair" role="group" aria-label="Sheet view">
            <button
              type="button"
              class="vault-editor-icon-btn"
              class:vault-editor-icon-btn--active={vault.ledgerEditMode === "table"}
              title="Table view"
              aria-label="Table view"
              aria-pressed={vault.ledgerEditMode === "table"}
              onclick={() => vault.setLedgerEditMode("table")}
            >
              <Table2 size={15} strokeWidth={1.75} />
            </button>
            <button
              type="button"
              class="vault-editor-icon-btn"
              class:vault-editor-icon-btn--active={vault.ledgerEditMode === "raw"}
              title="Raw markdown"
              aria-label="Raw markdown"
              aria-pressed={vault.ledgerEditMode === "raw"}
              onclick={() => vault.setLedgerEditMode("raw")}
            >
              <Code2 size={15} strokeWidth={1.75} />
            </button>
          </div>
        {/if}

        {#if showWorkbookViewToggle}
          <div class="vault-editor-icon-pair" role="group" aria-label="Workbook view">
            <button
              type="button"
              class="vault-editor-icon-btn"
              class:vault-editor-icon-btn--active={vault.workbookEditMode === "view"}
              title="Workbook view"
              aria-label="Workbook view"
              aria-pressed={vault.workbookEditMode === "view"}
              onclick={() => vault.setWorkbookEditMode("view")}
            >
              <BookOpen size={15} strokeWidth={1.75} />
            </button>
            <button
              type="button"
              class="vault-editor-icon-btn"
              class:vault-editor-icon-btn--active={vault.workbookEditMode === "raw"}
              title="Raw markdown"
              aria-label="Raw markdown"
              aria-pressed={vault.workbookEditMode === "raw"}
              onclick={() => vault.setWorkbookEditMode("raw")}
            >
              <Code2 size={15} strokeWidth={1.75} />
            </button>
          </div>
        {/if}

        {#if showBoardViewToggle}
          <div class="vault-editor-icon-pair" role="group" aria-label="Board view">
            <button
              type="button"
              class="vault-editor-icon-btn"
              class:vault-editor-icon-btn--active={vault.boardEditMode === "board"}
              title="Board view"
              aria-label="Board view"
              aria-pressed={vault.boardEditMode === "board"}
              onclick={() => vault.setBoardEditMode("board")}
            >
              <Columns3 size={15} strokeWidth={1.75} />
            </button>
            <button
              type="button"
              class="vault-editor-icon-btn"
              class:vault-editor-icon-btn--active={vault.boardEditMode === "raw"}
              title="Raw markdown"
              aria-label="Raw markdown"
              aria-pressed={vault.boardEditMode === "raw"}
              onclick={() => vault.setBoardEditMode("raw")}
            >
              <Code2 size={15} strokeWidth={1.75} />
            </button>
          </div>
        {/if}

        {#if showDeckViewToggle}
          <div class="vault-editor-icon-pair" role="group" aria-label="Deck view">
            <button
              type="button"
              class="vault-editor-icon-btn"
              class:vault-editor-icon-btn--active={vault.deckEditMode === "deck"}
              title="Deck view"
              aria-label="Deck view"
              aria-pressed={vault.deckEditMode === "deck"}
              onclick={() => vault.setDeckEditMode("deck")}
            >
              <BookOpen size={15} strokeWidth={1.75} />
            </button>
            <button
              type="button"
              class="vault-editor-icon-btn"
              class:vault-editor-icon-btn--active={vault.deckEditMode === "raw"}
              title="Raw markdown"
              aria-label="Raw markdown"
              aria-pressed={vault.deckEditMode === "raw"}
              onclick={() => vault.setDeckEditMode("raw")}
            >
              <Code2 size={15} strokeWidth={1.75} />
            </button>
          </div>
        {/if}

        {#if vault.selectedPath && vault.attachments.length > 0}
          <VaultLinkedFilesMenu disabled={vault.noteLoading || vault.saving} />
        {/if}

        <button
          type="button"
          class="vault-editor-icon-btn"
          title={titleWithShortcut("Find note", "open-notes")}
          aria-label="Find note"
          onclick={() => vaultQuickSwitcher.openSwitcher()}
        >
          <Search size={15} strokeWidth={1.75} />
        </button>

        <VaultEditorOverflowMenu
          selectedPath={vault.selectedPath}
          selectedKind={noteKind}
          editorMode={vault.editorMode}
          noteLoading={vault.noteLoading}
          saving={vault.saving}
          dirty={vault.dirty}
          saveStatus={vault.saveStatus}
          exportingPdf={exportingPdf}
          exportingWord={exportingWord}
          askSubmitting={workspace.askSubmitting}
          hasKanbanBoard={hasKanbanBoard}
          boardEditMode={vault.boardEditMode}
          linkedWork={linkedWork}
          showPreviewToggle={showPreviewButton}
          showSplitToggle={showSplitButton}
          splitEnabled={layout.vaultSplitEnabled}
          showLinksToggle={showLinksToggle}
          linksOpen={layout.vaultLinksPanelOpen}
          showEditSource={showNotePlaneToggle && isLivePlane}
          showBackToLive={showNotePlaneToggle && isBuildPlane}
          showEditorToggles={isBuildPlane && vault.editorMode === "edit"}
          buildWordWrap={vault.buildWordWrap}
          buildLineNumbers={vault.buildLineNumbers}
          buildAutoSave={vault.buildAutoSave}
          buildScrollSync={vault.buildScrollSync}
          monoSource={vault.editorSurface === "source"}
          {linkCount}
          onOpenChat={onOpenChat}
          onOpenWork={onOpenWork}
          onSelectCard={onSelectCard}
          onExportPdf={handleExportPdf}
          onExportWord={handleExportWord}
          onAskInChat={vault.isLooseFile ? undefined : handleAskInChatTab}
          onSendToWork={vault.isLooseFile ? undefined : handleSendToWork}
          onSave={handleSave}
          onOpenNoteActions={
            vault.isLooseFile ? undefined : () => vault.openNoteActions()
          }
          onOpenLooseMarkdown={() => void vault.openLooseMarkdownFile()}
          onInsertWeeklyReview={
            vault.isLooseFile ? undefined : () => vault.insertWeeklyReviewLink()
          }
          onPromoteJournal={
            vault.isLooseFile
              ? undefined
              : async () => {
                  await vault.promoteNote("journal");
                }
          }
          onPromoteProject={
            vault.isLooseFile
              ? undefined
              : async () => {
                  await vault.promoteNote("projects");
                }
          }
          onToggleBoard={
            vault.isLooseFile ? undefined : () => vault.toggleBoardEditMode()
          }
          onTogglePreview={() =>
            vault.editorMode === "edit"
              ? vault.enterPreviewMode()
              : vault.enterEditMode()}
          onToggleSplit={() => layout.toggleVaultSplitEnabled()}
          onToggleLinks={() => layout.toggleVaultLinksPanel()}
          onEditSource={() => {
            flushPendingEditorDrafts();
            vault.setNotePlane("build");
          }}
          onBackToLive={() => vault.setNotePlane("live")}
          onToggleWordWrap={() => vault.setBuildWordWrap(!vault.buildWordWrap)}
          onToggleLineNumbers={() =>
            vault.setBuildLineNumbers(!vault.buildLineNumbers)}
          onToggleAutoSave={() => vault.setBuildAutoSave(!vault.buildAutoSave)}
          onToggleScrollSync={() =>
            vault.setBuildScrollSync(!vault.buildScrollSync)}
          onToggleMonoSource={() => vault.toggleEditorSurface()}
          readingPaletteLabel={vault.readingPalette}
          onCycleReadingPalette={() => vault.cycleReadingPalette()}
          hideLiveMarkdownSyntax={vault.hideLiveMarkdownSyntax}
          onToggleHideLiveMarkdownSyntax={() =>
            vault.toggleHideLiveMarkdownSyntax()}
          paperWidthLabel={vault.paperWidth}
          onCyclePaperWidth={() => vault.cyclePaperWidth()}
          onFloatNote={canFloatSticky ? handleFloatSticky : undefined}
        />

        {#if !vault.isLooseFile && vault.selectedPath && noteKind !== "note"}
          <VaultKindBadge
            kind={noteKind}
            path={vault.selectedPath}
            interactive
            disabled={vault.noteLoading || vault.saving}
            onKindChange={(kind) => vault.setNoteKind(kind)}
          />
        {/if}
        {/if}
      </div>
    </header>
  {:else if mobile}
    <div class="shrink-0 border-b border-surface-500/40 px-4 py-2">
      {#if breadcrumb}
        <p class="workshop-faint truncate text-xs">{breadcrumb}</p>
      {/if}
      <h1 class="truncate text-sm font-semibold text-surface-50">{displayTitle}</h1>
    </div>
  {/if}

  {#if bound && vault.error}
    <p class="border-b border-error-500/30 bg-error-500/10 px-4 py-2 text-xs text-content-error">
      {vault.error}
    </p>
  {/if}

  {#if bound}
  <VaultProposalBar {mobile} />
  <VaultConflictBar />
  {/if}

  {#if bound && showLinksToggle && !layout.vaultLinksPanelOpen && !mobile}
    <div class="flex shrink-0 items-center gap-2 border-b border-surface-500/30 px-4 py-1.5 text-xs">
      <span class="text-content-quiet">{linkCount} linked note{linkCount === 1 ? "" : "s"}</span>
      <button
        type="button"
        class="text-content-link hover:text-primary-200"
        onclick={() => layout.setVaultLinksPanelOpen(true)}
      >
        Show links
      </button>
    </div>
  {/if}

  {#if !notePath}
    <VaultEmptyState />
  {:else if !liveHost}
    <div class="relative flex min-h-0 min-w-0 max-w-full flex-1 overflow-hidden">
      {#if displayLoading}
        <div
          class="absolute inset-0 z-10 flex items-center justify-center bg-surface-950/50 text-sm text-content-tertiary"
        >
          Loading note…
        </div>
      {/if}
      <VaultMarkdownPreview
        content={displayContent}
        {labelByPath}
        onWikilink={undefined}
      />
      {#if interactive && bound}
        <VaultVersionsPanel />
      {/if}
    </div>
  {:else}
    <div class="relative flex min-h-0 min-w-0 max-w-full flex-1 overflow-hidden">
      {#if displayLoading}
        <div
          class="absolute inset-0 z-10 flex items-center justify-center bg-surface-950/50 text-sm text-content-tertiary"
        >
          Loading note…
        </div>
      {/if}
      <div class="relative flex min-h-0 min-w-0 max-w-full flex-1 flex-col overflow-hidden">
        <div class="flex min-h-0 min-w-0 max-w-full flex-1 flex-col overflow-hidden">
        {#if showLedgerTable}
          <LedgerTableEditor
            content={displayContent}
            disabled={!interactive || vault.saving}
            onchange={(next) => vault.markDirty(next, { path: notePath })}
          />
        {:else if showWorkbookManifest}
          <WorkbookManifestEditor
            content={displayContent}
            disabled={!interactive || vault.saving}
            onchange={(next) => vault.markDirty(next, { path: notePath })}
          />
        {:else if showKanbanBoard}
          <KanbanBoardEditor
            content={displayContent}
            disabled={!interactive || vault.saving || vault.editorMode === "preview"}
            onchange={(next) => vault.markDirty(next, { path: notePath })}
            onWikilink={handleWikilink}
          />
        {:else if showSlidesDeck}
          <SlidesDeckEditor
            bind:this={slidesDeckEl}
            content={displayContent}
            disabled={!interactive || vault.saving || vault.editorMode === "preview"}
            onchange={(next) => vault.markDirty(next, { path: notePath })}
          />
        {:else if showDrawSurface}
          <DrawNoteEditor
            content={displayContent}
            disabled={!interactive || vault.saving}
            onchange={(next) => vault.markDirty(next, { path: notePath })}
          />
        {:else if showMarkdownEditor}
          <VaultMarkdownEditor
            bind:this={markdownEditorEl}
            content={displayContent}
            contentSyncKey={displaySyncKey}
            {displayTitle}
            disabled={!bound || !interactive || displayLoading}
            class="flex-1"
            surface={editorSurface}
            showFormatChrome={bound && isBuildPlane}
            split={bound && showSplitEditor}
            splitWidth={layout.vaultEditorPaneWidth}
            onSplitResize={(width) => layout.setVaultEditorPaneWidth(width)}
            previewScrollEl={previewScrollEl}
            scrollSyncEnabled={vault.buildScrollSync}
            onchange={(next) => {
              if (!bound) return;
              vault.markDirty(next, { path: notePath });
            }}
            showFloat={canFloatSticky}
            onFloat={() => void handleFloatSticky()}
          >
            {#snippet preview()}
              <VaultMarkdownPreview
                content={displayContent}
                {labelByPath}
                compact
                bind:scrollEl={previewScrollEl}
                onWikilink={vault.isLooseFile ? undefined : handleWikilink}
                onHeadingClick={(heading) => markdownEditorEl?.scrollToHeadingSource(heading)}
              />
            {/snippet}
          </VaultMarkdownEditor>
        {:else if showPreviewOnly}
          <VaultMarkdownPreview
            content={displayContent}
            {labelByPath}
            bind:scrollEl={previewScrollEl}
            onWikilink={vault.isLooseFile ? undefined : handleWikilink}
          />
        {/if}
        </div>
        {#if findSupported && vaultFind.open}
          <VaultFindBar />
        {/if}
        {#if showHeadingOutline && outlineMode === "rail"}
          <MarkdownHeadingOutline
            items={outlineItems}
            activeId={outlineActiveId}
            scrolling={outlineScrolling}
            mode="rail"
            showModeToggle
            onSelect={jumpToOutlineHeading}
            onToggleMode={toggleOutlineMode}
          />
        {/if}
      </div>

      {#if showHeadingOutline && outlineMode === "panel"}
        <MarkdownHeadingOutline
          items={outlineItems}
          activeId={outlineActiveId}
          mode="panel"
          showModeToggle
          onSelect={jumpToOutlineHeading}
          onToggleMode={toggleOutlineMode}
        />
      {/if}

      {#if showLinksPanel}
        <VaultNoteLinksPanel
          wikilinksOut={vault.wikilinksOut}
          backlinks={vault.backlinks}
          {labelByPath}
          onOpenNote={(path) => vault.openNote(path)}
        />
      {/if}
      {#if interactive && bound}
        <VaultVersionsPanel />
      {/if}
    </div>
  {/if}

  {#if interactive && bound && vault.selectedPath && !mobile && !noteWorkshop.open && !vault.isLooseFile && environment.desktopShellChrome.vaultChatFab}
    <VaultNoteChatFab />
  {/if}
</section>

{#if interactive && bound}
  <VaultNoteActionsMenu />
  <VaultViewBuilderSheet
    open={vault.viewBridgeOpen}
    mode={vault.viewBridgeMode}
    initialQuery={vault.viewBridgeQuery}
    onSave={(query) => vault.commitViewBridge(query)}
    onClose={() => vault.closeViewBridge()}
  />
  <VaultChartBuilderSheet
    open={vault.chartBridgeOpen}
    initialKv={vault.chartBridgeKv}
    initialTableMarkdown={vault.chartBridgeTableMarkdown}
    onSave={(kv, tableMarkdown) => vault.commitChartBridge(kv, tableMarkdown)}
    onClose={() => vault.closeChartBridge()}
  />
  <VaultLiquidBuilderSheet
    open={vault.liquidBridgeOpen}
    lang={vault.liquidBridgeLang}
    initial={vault.liquidBridgeDraft}
    onSave={(next) => vault.commitLiquidBridge(next)}
    onClose={() => vault.closeLiquidBridge()}
  />
  <LiquidCardDetailSheet
    open={vault.cardDetailOpen}
    detail={vault.cardDetail}
    onClose={() => vault.closeCardDetail()}
  />
  <VaultExportPreviewModal
    open={exportPreviewOpen}
    title={exportPreviewTitle}
    content={exportPreviewContent}
    labelByPath={exportPreviewLabels}
    notePath={exportPreviewPath}
    initialFormat={exportPreviewFormat}
    onClose={handleExportPreviewClose}
    onPreparingChange={(preparing) => {
      // Format can switch inside the modal — reflect busy state on both labels.
      exportingPdf = preparing;
      exportingWord = preparing;
    }}
  />
{/if}
