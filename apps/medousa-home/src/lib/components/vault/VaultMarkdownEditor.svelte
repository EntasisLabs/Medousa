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
  import VaultLiveEditor from "./VaultLiveEditor.svelte";
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
  import type { SlashMenuAnchor } from "$lib/utils/slashMenuPlacement";

  interface Props {
    content: string;
    contentSyncKey: string;
    /** Header title for Live duplicate-H1 collapse. */
    displayTitle?: string;
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
    /** Build plane shows the format bar; Live hides it (slash/shortcuts remain). */
    showFormatChrome?: boolean;
    showFloat?: boolean;
    onFloat?: () => void;
  }

  let {
    content,
    contentSyncKey,
    displayTitle = "",
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
    showFormatChrome = true,
    showFloat = false,
    onFloat,
  }: Props = $props();

  let cmEl = $state<ReturnType<typeof VaultMarkdownCodeMirror> | null>(null);
  let liveEl = $state<ReturnType<typeof VaultLiveEditor> | null>(null);
  let editorShellEl = $state<HTMLElement | null>(null);
  const scrollSync = createVaultScrollSync();
  let slashMenuEl = $state<ReturnType<typeof VaultSlashMenu> | null>(null);
  // Seed from content so CM doesn't mount on an empty draft during hydrate.
  let draft = $state(content);
  let syncedKey = $state(contentSyncKey);
  let selectionStart = $state(0);
  let selectionEnd = $state(0);
  let slashOpen = $state(false);
  let slashAnchor = $state<SlashMenuAnchor | null>(null);
  let notePickerOpen = $state(false);
  let notePickerMode = $state<"wikilink" | "embed">("wikilink");
  let calloutBuilderOpen = $state(false);
  let chartTypePickerOpen = $state(false);
  let bridgeInsertAt = $state(0);
  let activeActions = $state<MarkdownFormatAction[]>([]);
  let lastPlane = $state<"live" | "build" | null>(null);
  let liveSlashFilter = $state("");

  const isLivePlane = $derived(vault.notePlane === "live");
  const slashFilter = $derived(
    isLivePlane ? liveSlashFilter : slashMenuFilter(draft, selectionStart),
  );

  /**
   * Document identity changed → adopt vault.content before {#key} remounts.
   * Must be `pre` so Live/Build never mount on the previous note's draft.
   */
  $effect.pre(() => {
    if (contentSyncKey !== syncedKey) {
      draft = content;
      syncedKey = contentSyncKey;
      slashOpen = false;
      slashAnchor = null;
      liveSlashFilter = "";
    }
  });

  /**
   * Flush the leaving plane into draft/vault BEFORE the editor unmounts.
   * Live must never flush in onDestroy (that races note switches).
   */
  $effect.pre(() => {
    const plane = vault.notePlane;
    if (lastPlane === null) {
      lastPlane = plane;
      return;
    }
    if (lastPlane === plane) return;

    if (lastPlane === "live" && plane === "build") {
      const flushed = liveEl?.flush();
      if (flushed != null) {
        draft = flushed;
        onchange(flushed);
      }
    } else if (lastPlane === "build" && plane === "live") {
      const fromCm = cmEl?.getContent();
      if (fromCm != null) {
        draft = fromCm;
        onchange(fromCm);
      }
    }
    slashOpen = false;
    slashAnchor = null;
    liveSlashFilter = "";
    lastPlane = plane;
  });

  $effect(() => {
    vault.setCompositionHold(slashOpen);
  });

  $effect(() => {
    vault.editorInsertRequest;
    const insert = vault.takeEditorInsert();
    if (!insert) return;
    if (isLivePlane) {
      if (!liveEl) return;
      liveEl.insertText(insert);
      return;
    }
    if (!cmEl) return;
    const { start } = cmEl.getSelection();
    void applyEdit(insertTextAtCursor(draft, start, insert));
  });

  $effect(() => {
    vaultFind.registerReplaceHandler((result) => {
      if (isLivePlane) return;
      void applyEdit(result);
    });
    vaultFind.registerHighlightHandler((matches, activeIndex) => {
      if (isLivePlane) return;
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
    if (!slashOpen) {
      slashAnchor = null;
      return;
    }
    if (isLivePlane) {
      slashAnchor = liveEl?.getSlashAnchor(editorShellEl) ?? null;
      return;
    }
    if (!cmEl) {
      slashAnchor = null;
      return;
    }
    slashAnchor = cmEl.getSlashAnchor(editorShellEl);
  }

  function syncSlashMenu() {
    if (isLivePlane) {
      slashOpen = liveEl?.isSlashOpen() ?? false;
      liveSlashFilter = liveEl?.slashFilter() ?? "";
      if (slashOpen) updateSlashAnchor();
      else slashAnchor = null;
      return;
    }
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

  function handleLiveChange(next: string) {
    if (next === draft) return;
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
    if (!path) return;
    if (isLivePlane) {
      handleVaultNoteContextMenuEvent(path, event, null);
      return;
    }
    if (!cmEl) return;
    const { start, end } = cmEl.getSelection();
    const text = draft.slice(Math.min(start, end), Math.max(start, end));
    handleVaultNoteContextMenuEvent(
      path,
      event,
      text.trim() ? { text, start, end } : null,
    );
  }

  async function clearSlashAndRememberInsert(): Promise<number> {
    if (isLivePlane) {
      liveEl?.clearSlash();
      bridgeInsertAt = 0;
      return 0;
    }
    const cleared = replaceSlashWith(draft, selectionStart, "");
    applyEdit(cleared);
    bridgeInsertAt = cleared.selectionStart;
    return cleared.selectionStart;
  }

  function handleSlashSelect(block: SlashBlockId) {
    if (isLivePlane) {
      if (!liveEl) return;
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
        void clearSlashAndRememberInsert().then(() => {
          const el = liveEl;
          if (!el) return;
          const flushed = el.flush();
          draft = flushed;
          onchange(flushed);
          vault.openViewBridgeInsert(flushed.length);
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
      // Live: chart arrives as a living figure — pick type on the organism (no modal).
      liveEl.applySlash(block);
      slashOpen = false;
      return;
    }

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
    if (isLivePlane) {
      if (!liveEl) return;
      if (notePickerMode === "embed") {
        liveEl.clearSlash();
        liveEl.insertEmbed(path);
        notePickerOpen = false;
        return;
      }
      liveEl.clearSlash();
      const label =
        vault.labelByPath().get(path) ??
        vaultDisplayTitle(path.split("/").pop()?.replace(/\.md$/i, "") ?? path, path);
      liveEl.insertWikilink(path, label.trim() || path);
      notePickerOpen = false;
      return;
    }
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
    if (isLivePlane) {
      if (markdown.trimStart().startsWith("```")) {
        liveEl?.insertFence(markdown);
      } else {
        liveEl?.insertText(markdown);
      }
      return;
    }
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
    if (isLivePlane) return;
    cmEl?.scrollToHeadingSource(headingText);
  }

  export function flushLive(): string {
    if (!isLivePlane) return draft;
    const flushed = liveEl?.flush() ?? draft;
    draft = flushed;
    return flushed;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="vault-markdown-editor vault-markdown-editor--{surface} relative flex min-h-0 flex-1 flex-col {className}"
  class:vault-markdown-editor--live={isLivePlane || !showFormatChrome}
  oncontextmenu={handleContextMenu}
>
  {#if showFormatChrome && !isLivePlane}
    <VaultFormatBar
      {disabled}
      compact={formatCompact}
      {showFloat}
      {onFloat}
      {activeActions}
      onFormat={handleFormat}
      onColor={handleColor}
    />
  {/if}
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
    {#if split && preview && onSplitResize && !isLivePlane}
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
          {#key contentSyncKey}
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
          {/key}
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
        {#key contentSyncKey}
          {#if isLivePlane}
            <VaultLiveEditor
              bind:this={liveEl}
              value={draft}
              {contentSyncKey}
              {displayTitle}
              {disabled}
              {slashOpen}
              onchange={handleLiveChange}
              onSlashCheck={syncSlashMenu}
              onSlashKey={handleSlashKey}
            />
          {:else}
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
          {/if}
        {/key}
      </div>
    {/if}
  </div>
</div>
