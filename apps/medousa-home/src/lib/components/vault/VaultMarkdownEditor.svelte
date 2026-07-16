<script lang="ts">
  import { type Snippet } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import VaultFormatBar from "./VaultFormatBar.svelte";
  import VaultSlashMenu from "./VaultSlashMenu.svelte";
  import VaultNotePicker from "./VaultNotePicker.svelte";
  import VaultCalloutBuilderSheet from "./VaultCalloutBuilderSheet.svelte";
  import VaultChartTypePicker from "./VaultChartTypePicker.svelte";
  import VaultMarkdownCodeMirror from "./VaultMarkdownCodeMirror.svelte";
  import {
    insertSlashBlock,
    insertTextAtCursor,
    insertVaultWikilink,
    replaceSlashWith,
    serializeTransclusion,
    shouldOpenSlashMenu,
    slashMenuFilter,
    type EditResult,
    type MarkdownFormatAction,
    type MarkdownColorToken,
    type SlashBlockId,
  } from "$lib/utils/vaultMarkdownEdit";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { handleVaultNoteContextMenuEvent } from "$lib/utils/vaultContextMenuEvents";
  import { createVaultScrollSync } from "$lib/utils/vaultScrollSync";

  interface Props {
    content: string;
    contentSyncKey: string;
    disabled?: boolean;
    class?: string;
    onchange: (next: string) => void;
    surface?: "write" | "source";
    /** Live preview pane beside the source editor (split view). */
    split?: boolean;
    splitWidth?: number;
    splitMin?: number;
    splitMax?: number;
    onSplitResize?: (width: number) => void;
    /** Preview scroll container for bidirectional sync when split is on. */
    previewScrollEl?: HTMLElement | null;
    preview?: Snippet;
    formatCompact?: boolean;
    showFloat?: boolean;
    onFloat?: () => void;
  }

  let {
    content,
    contentSyncKey,
    disabled = false,
    class: className = "",
    onchange,
    surface = "write",
    split = false,
    splitWidth = 420,
    splitMin = 280,
    splitMax = 720,
    onSplitResize,
    previewScrollEl = null,
    preview,
    formatCompact = false,
    showFloat = false,
    onFloat,
  }: Props = $props();

  let cmEl = $state<ReturnType<typeof VaultMarkdownCodeMirror> | null>(null);
  let editorShellEl = $state<HTMLElement | null>(null);
  const scrollSync = createVaultScrollSync();
  let slashMenuEl = $state<ReturnType<typeof VaultSlashMenu> | null>(null);
  // Seed from content so CM doesn't mount on an empty draft during hydrate.
  let draft = $state(content);
  let syncedKey = $state(contentSyncKey);
  let selectionStart = $state(0);
  let selectionEnd = $state(0);
  let slashOpen = $state(false);
  let slashAnchor = $state<{ top: number; left: number } | null>(null);
  let notePickerOpen = $state(false);
  let notePickerMode = $state<"wikilink" | "embed">("wikilink");
  let calloutBuilderOpen = $state(false);
  let chartTypePickerOpen = $state(false);
  let bridgeInsertAt = $state(0);
  let activeActions = $state<MarkdownFormatAction[]>([]);

  const slashFilter = $derived(slashMenuFilter(draft, selectionStart));

  $effect(() => {
    if (contentSyncKey !== syncedKey) {
      draft = content;
      syncedKey = contentSyncKey;
    }
  });

  $effect(() => {
    vault.setCompositionHold(slashOpen);
  });

  $effect(() => {
    vault.editorInsertRequest;
    const insert = vault.takeEditorInsert();
    if (!insert || !cmEl) return;
    const { start } = cmEl.getSelection();
    void applyEdit(insertTextAtCursor(draft, start, insert));
  });

  $effect(() => {
    vaultFind.registerReplaceHandler((result) => {
      void applyEdit(result);
    });
    vaultFind.registerHighlightHandler((matches, activeIndex) => {
      cmEl?.refreshFindHighlights(matches, activeIndex);
    });
    return () => {
      vaultFind.registerReplaceHandler(null);
      vaultFind.registerHighlightHandler(null);
    };
  });

  $effect(() => {
    if (!vaultFind.open || vault.editorMode !== "edit") return;
    draft;
    vaultFind.setSourceText(draft);
  });

  function handleCmScroll() {
    const scrollEl = cmEl?.getScrollEl();
    if (split && scrollEl && previewScrollEl) {
      scrollSync.sync(scrollEl, previewScrollEl);
    }
    if (slashOpen) updateSlashAnchor();
  }

  $effect(() => {
    const scrollEl = cmEl?.getScrollEl();
    if (!scrollEl) return;
    const onScroll = () => handleCmScroll();
    scrollEl.addEventListener("scroll", onScroll, { passive: true });
    return () => scrollEl.removeEventListener("scroll", onScroll);
  });

  $effect(() => {
    if (!split || !previewScrollEl) return;
    const previewEl = previewScrollEl;
    const onPreviewScroll = () => {
      const scrollEl = cmEl?.getScrollEl();
      if (scrollEl) scrollSync.sync(previewEl, scrollEl);
    };
    previewEl.addEventListener("scroll", onPreviewScroll, { passive: true });
    return () => previewEl.removeEventListener("scroll", onPreviewScroll);
  });

  function updateSlashAnchor() {
    if (!cmEl || !slashOpen) {
      slashAnchor = null;
      return;
    }
    slashAnchor = cmEl.getSlashAnchor(editorShellEl);
  }

  function syncSlashMenu() {
    if (!cmEl) {
      slashOpen = false;
      slashAnchor = null;
      return;
    }
    const { start } = cmEl.getSelection();
    slashOpen = shouldOpenSlashMenu(draft, start);
    if (slashOpen) updateSlashAnchor();
    else slashAnchor = null;
  }

  function applyEdit(result: EditResult) {
    cmEl?.applyEdit(result);
    draft = result.content;
    onchange(result.content);
    selectionStart = result.selectionStart;
    selectionEnd = result.selectionEnd;
    activeActions = cmEl?.getActiveFormats() ?? [];
    syncSlashMenu();
  }

  function handleFormat(action: MarkdownFormatAction) {
    cmEl?.format(action);
    draft = cmEl?.getContent() ?? draft;
    onchange(draft);
    const sel = cmEl?.getSelection();
    if (sel) {
      selectionStart = sel.start;
      selectionEnd = sel.end;
    }
    activeActions = cmEl?.getActiveFormats() ?? [];
    syncSlashMenu();
  }

  function handleColor(color: MarkdownColorToken) {
    cmEl?.color(color);
    draft = cmEl?.getContent() ?? draft;
    onchange(draft);
    const sel = cmEl?.getSelection();
    if (sel) {
      selectionStart = sel.start;
      selectionEnd = sel.end;
    }
    syncSlashMenu();
  }

  function handleContentChange(next: string) {
    draft = next;
    onchange(next);
    syncSlashMenu();
  }

  function handleSelectionChange(start: number, end: number) {
    selectionStart = start;
    selectionEnd = end;
    activeActions = cmEl?.getActiveFormats() ?? [];
    syncSlashMenu();
  }

  function handleContextMenu(event: MouseEvent) {
    const path = vault.selectedPath;
    if (!path || !cmEl) return;
    const { start, end } = cmEl.getSelection();
    const text = draft.slice(Math.min(start, end), Math.max(start, end));
    handleVaultNoteContextMenuEvent(
      path,
      event,
      text.trim() ? { text, start, end } : null,
    );
  }

  async function clearSlashAndRememberInsert(): Promise<number> {
    const cleared = replaceSlashWith(draft, selectionStart, "");
    applyEdit(cleared);
    bridgeInsertAt = cleared.selectionStart;
    return cleared.selectionStart;
  }

  function handleSlashSelect(block: SlashBlockId) {
    if (!cmEl) return;
    if (block === "wikilink") {
      slashOpen = false;
      notePickerMode = "wikilink";
      notePickerOpen = true;
      return;
    }
    if (block === "embed") {
      slashOpen = false;
      void clearSlashAndRememberInsert().then(() => {
        notePickerMode = "embed";
        notePickerOpen = true;
      });
      return;
    }
    if (block === "view") {
      slashOpen = false;
      void clearSlashAndRememberInsert().then((insertAt) => {
        vault.openViewBridgeInsert(insertAt);
      });
      return;
    }
    if (block === "callout") {
      slashOpen = false;
      void clearSlashAndRememberInsert().then(() => {
        calloutBuilderOpen = true;
      });
      return;
    }
    if (block === "liquid_chart") {
      slashOpen = false;
      void clearSlashAndRememberInsert().then(() => {
        chartTypePickerOpen = true;
      });
      return;
    }
    const result = insertSlashBlock(draft, selectionStart, block);
    slashOpen = false;
    applyEdit(result);
  }

  function handleNotePick(path: string) {
    if (!cmEl) return;
    if (notePickerMode === "embed") {
      const result = insertTextAtCursor(
        draft,
        bridgeInsertAt,
        serializeTransclusion(path),
      );
      notePickerOpen = false;
      applyEdit(result);
      return;
    }
    const label =
      vault.labelByPath().get(path) ??
      vaultDisplayTitle(path.split("/").pop()?.replace(/\.md$/i, "") ?? path, path);
    const result = insertVaultWikilink(draft, selectionStart, path, label);
    notePickerOpen = false;
    applyEdit(result);
  }

  function handleBridgeInsert(markdown: string) {
    applyEdit(insertTextAtCursor(draft, bridgeInsertAt, markdown));
  }

  function closeSlashMenu() {
    slashOpen = false;
    slashAnchor = null;
  }

  function handleSlashKey(key: string): boolean {
    return slashMenuEl?.handleMenuKey(key) ?? false;
  }

  export function scrollToHeadingSource(headingText: string) {
    cmEl?.scrollToHeadingSource(headingText);
  }

</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="vault-markdown-editor vault-markdown-editor--{surface} relative flex min-h-0 flex-1 flex-col {className}"
  oncontextmenu={handleContextMenu}
>
  <VaultFormatBar
    {disabled}
    compact={formatCompact}
    {showFloat}
    {onFloat}
    {surface}
    {activeActions}
    onToggleSurface={() => vault.toggleEditorSurface()}
    onFormat={handleFormat}
    onColor={handleColor}
  />
  <VaultNotePicker
    open={notePickerOpen}
    onSelect={handleNotePick}
    onClose={() => (notePickerOpen = false)}
  />
  <VaultCalloutBuilderSheet
    open={calloutBuilderOpen}
    onInsert={handleBridgeInsert}
    onClose={() => (calloutBuilderOpen = false)}
  />
  <VaultChartTypePicker
    open={chartTypePickerOpen}
    onInsert={handleBridgeInsert}
    onClose={() => (chartTypePickerOpen = false)}
  />

  <div class="flex min-h-0 flex-1">
    {#if split && preview && onSplitResize}
      <SplitPane
        width={splitWidth}
        side="left"
        min={splitMin}
        max={splitMax}
        onResize={onSplitResize}
      >
        <div
          bind:this={editorShellEl}
          class="vault-find-editor-shell relative flex min-h-0 flex-1 flex-col"
        >
          <VaultSlashMenu
            bind:this={slashMenuEl}
            open={slashOpen}
            filter={slashFilter}
            anchor={slashAnchor}
            onSelect={handleSlashSelect}
            onClose={closeSlashMenu}
          />
          <VaultMarkdownCodeMirror
            bind:this={cmEl}
            value={draft}
            {contentSyncKey}
            {disabled}
            {surface}
            {slashOpen}
            onchange={handleContentChange}
            onSelectionChange={handleSelectionChange}
            onSlashCheck={syncSlashMenu}
            onSlashKey={handleSlashKey}
          />
        </div>
      </SplitPane>
      {@render preview()}
    {:else}
      <div
        bind:this={editorShellEl}
        class="vault-find-editor-shell relative flex min-h-0 flex-1 flex-col"
      >
        <VaultSlashMenu
          bind:this={slashMenuEl}
          open={slashOpen}
          filter={slashFilter}
          anchor={slashAnchor}
          onSelect={handleSlashSelect}
          onClose={closeSlashMenu}
        />
        <VaultMarkdownCodeMirror
          bind:this={cmEl}
          value={draft}
          {contentSyncKey}
          {disabled}
          {surface}
          {slashOpen}
          onchange={handleContentChange}
          onSelectionChange={handleSelectionChange}
          onSlashCheck={syncSlashMenu}
          onSlashKey={handleSlashKey}
        />
      </div>
    {/if}
  </div>
</div>
