<script lang="ts">
  import { tick, type Snippet } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import { syncTextareaFindScroll } from "$lib/utils/vaultFindInNote";
  import VaultFormatBar from "./VaultFormatBar.svelte";
  import VaultSlashMenu from "./VaultSlashMenu.svelte";
  import VaultNotePicker from "./VaultNotePicker.svelte";
  import VaultCalloutBuilderSheet from "./VaultCalloutBuilderSheet.svelte";
  import {
    applyMarkdownFormat,
    applyMarkdownColor,
    insertSlashBlock,
    insertTextAtCursor,
    insertVaultWikilink,
    replaceSlashWith,
    serializeTransclusion,
    shouldOpenSlashMenu,
    slashMenuFilter,
    type MarkdownFormatAction,
    type MarkdownColorToken,
    type SlashBlockId,
  } from "$lib/utils/vaultMarkdownEdit";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";

  interface Props {
    content: string;
    contentSyncKey: string;
    disabled?: boolean;
    class?: string;
    onchange: (next: string) => void;
    surface?: "write" | "source";
    /** Live preview pane beside the textarea (split view). */
    split?: boolean;
    splitWidth?: number;
    splitMin?: number;
    splitMax?: number;
    onSplitResize?: (width: number) => void;
    preview?: Snippet;
  }

  let {
    content,
    contentSyncKey,
    disabled = false,
    class: className = "",
    onchange,
    surface = "source",
    split = false,
    splitWidth = 420,
    splitMin = 280,
    splitMax = 720,
    onSplitResize,
    preview,
  }: Props = $props();

  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let textareaBackdropEl = $state<HTMLElement | null>(null);
  let slashMenuEl = $state<ReturnType<typeof VaultSlashMenu> | null>(null);
  let draft = $state("");
  let syncedKey = $state("");
  let selectionStart = $state(0);
  let selectionEnd = $state(0);
  let slashOpen = $state(false);
  let notePickerOpen = $state(false);
  let notePickerMode = $state<"wikilink" | "embed">("wikilink");
  let calloutBuilderOpen = $state(false);
  let bridgeInsertAt = $state(0);

  const slashFilter = $derived(
    textareaEl ? slashMenuFilter(textareaEl.value, textareaEl.selectionStart) : "",
  );

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
    if (!insert || !textareaEl) return;
    captureSelection();
    const result = insertTextAtCursor(draft, selectionStart, insert);
    void applyEdit(result);
  });

  $effect(() => {
    vaultFind.registerTextarea(textareaEl);
    return () => vaultFind.registerTextarea(null);
  });

  $effect(() => {
    vaultFind.registerTextareaBackdrop(textareaBackdropEl);
    return () => vaultFind.registerTextareaBackdrop(null);
  });

  $effect(() => {
    if (!vaultFind.open || vault.editorMode !== "edit") return;
    draft;
    vaultFind.setSourceText(draft);
  });

  function syncFindScroll() {
    if (!textareaEl || !textareaBackdropEl || textareaBackdropEl.hidden) return;
    syncTextareaFindScroll(textareaEl, textareaBackdropEl);
  }

  function captureSelection() {
    if (!textareaEl) return;
    selectionStart = textareaEl.selectionStart;
    selectionEnd = textareaEl.selectionEnd;
  }

  function syncSlashMenu() {
    if (!textareaEl) {
      slashOpen = false;
      return;
    }
    slashOpen = shouldOpenSlashMenu(textareaEl.value, textareaEl.selectionStart);
  }

  async function applyEdit(result: {
    content: string;
    selectionStart: number;
    selectionEnd: number;
  }) {
    draft = result.content;
    onchange(result.content);
    await tick();
    if (!textareaEl) return;
    textareaEl.focus();
    textareaEl.setSelectionRange(result.selectionStart, result.selectionEnd);
    selectionStart = result.selectionStart;
    selectionEnd = result.selectionEnd;
    syncSlashMenu();
  }

  function handleFormat(action: MarkdownFormatAction) {
    if (!textareaEl) return;
    captureSelection();
    const result = applyMarkdownFormat(draft, selectionStart, selectionEnd, action);
    void applyEdit(result);
  }

  function handleColor(color: MarkdownColorToken) {
    if (!textareaEl) return;
    captureSelection();
    const result = applyMarkdownColor(draft, selectionStart, selectionEnd, color);
    void applyEdit(result);
  }

  function handleInput() {
    onchange(draft);
    if (!textareaEl) return;
    selectionStart = textareaEl.selectionStart;
    selectionEnd = textareaEl.selectionEnd;
    syncSlashMenu();
  }

  async function clearSlashAndRememberInsert(): Promise<number> {
    captureSelection();
    const cleared = replaceSlashWith(draft, selectionStart, "");
    await applyEdit(cleared);
    bridgeInsertAt = cleared.selectionStart;
    return cleared.selectionStart;
  }

  function handleSlashSelect(block: SlashBlockId) {
    if (!textareaEl) return;
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
    captureSelection();
    const result = insertSlashBlock(draft, selectionStart, block);
    slashOpen = false;
    void applyEdit(result);
  }

  function handleNotePick(path: string) {
    if (!textareaEl) return;
    if (notePickerMode === "embed") {
      const result = insertTextAtCursor(
        draft,
        bridgeInsertAt,
        serializeTransclusion(path),
      );
      notePickerOpen = false;
      void applyEdit(result);
      return;
    }
    captureSelection();
    const label =
      vault.labelByPath().get(path) ??
      vaultDisplayTitle(path.split("/").pop()?.replace(/\.md$/i, "") ?? path, path);
    const result = insertVaultWikilink(draft, selectionStart, path, label);
    notePickerOpen = false;
    void applyEdit(result);
  }

  function handleBridgeInsert(markdown: string) {
    const result = insertTextAtCursor(draft, bridgeInsertAt, markdown);
    void applyEdit(result);
  }

  function closeSlashMenu() {
    slashOpen = false;
  }

  function handleKeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      event.stopPropagation();
      vaultFind.setSourceText(draft);
      vaultFind.openFind();
      return;
    }
    if (slashMenuEl?.handleMenuKeydown(event)) {
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
      event.preventDefault();
      textareaEl?.select();
      captureSelection();
      return;
    }
    if (event.key === "Escape" && slashOpen) {
      event.preventDefault();
      closeSlashMenu();
    }
  }

  const textareaClass =
    "vault-editor-textarea vault-find-editor-input textarea h-full w-full resize-none rounded-none border-0 bg-surface-950 text-sm leading-relaxed";
  const backdropClass =
    "vault-find-editor-backdrop vault-editor-textarea textarea h-full w-full resize-none rounded-none border-0 bg-surface-950 text-sm leading-relaxed";
</script>

<div
  class="vault-markdown-editor vault-markdown-editor--{surface} relative flex min-h-0 flex-1 flex-col {className}"
>
  <VaultFormatBar {disabled} onFormat={handleFormat} onColor={handleColor} />
  <VaultSlashMenu
    bind:this={slashMenuEl}
    open={slashOpen}
    filter={slashFilter}
    onSelect={handleSlashSelect}
    onClose={closeSlashMenu}
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

  <div class="flex min-h-0 flex-1">
    {#if split && preview && onSplitResize}
      <SplitPane
        width={splitWidth}
        side="left"
        min={splitMin}
        max={splitMax}
        onResize={onSplitResize}
      >
        <div class="vault-find-editor-shell relative flex min-h-0 flex-1 flex-col">
          <div
            bind:this={textareaBackdropEl}
            class={backdropClass}
            hidden
            aria-hidden="true"
          ></div>
          <textarea
            bind:this={textareaEl}
            class="{textareaClass} relative z-[1]"
            bind:value={draft}
            {disabled}
            oninput={handleInput}
            onkeydown={handleKeydown}
            onscroll={syncFindScroll}
            onselect={() => {
              captureSelection();
              syncSlashMenu();
            }}
            onkeyup={() => {
              captureSelection();
              syncSlashMenu();
            }}
            onmouseup={() => {
              captureSelection();
              syncSlashMenu();
            }}
            onclick={captureSelection}
          ></textarea>
        </div>
      </SplitPane>
      {@render preview()}
    {:else}
      <div class="vault-find-editor-shell relative flex min-h-0 flex-1 flex-col">
        <div
          bind:this={textareaBackdropEl}
          class="{backdropClass} flex-1"
          hidden
          aria-hidden="true"
        ></div>
        <textarea
          bind:this={textareaEl}
          class="{textareaClass} relative z-[1] flex-1"
          bind:value={draft}
          {disabled}
          oninput={handleInput}
          onkeydown={handleKeydown}
          onscroll={syncFindScroll}
          onselect={() => {
            captureSelection();
            syncSlashMenu();
          }}
          onkeyup={() => {
            captureSelection();
            syncSlashMenu();
          }}
          onmouseup={() => {
            captureSelection();
            syncSlashMenu();
          }}
          onclick={captureSelection}
        ></textarea>
      </div>
    {/if}
  </div>
</div>
