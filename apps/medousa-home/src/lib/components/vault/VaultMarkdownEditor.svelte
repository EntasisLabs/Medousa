<script lang="ts">
  import { tick, type Snippet } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import VaultFormatBar from "./VaultFormatBar.svelte";
  import VaultSlashMenu from "./VaultSlashMenu.svelte";
  import VaultNotePicker from "./VaultNotePicker.svelte";
  import {
    applyMarkdownFormat,
    applyMarkdownColor,
    insertSlashBlock,
    insertVaultWikilink,
    shouldOpenSlashMenu,
    slashMenuFilter,
    type MarkdownFormatAction,
    type MarkdownColorId,
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
  let slashMenuEl = $state<ReturnType<typeof VaultSlashMenu> | null>(null);
  let draft = $state("");
  let syncedKey = $state("");
  let selectionStart = $state(0);
  let selectionEnd = $state(0);
  let slashOpen = $state(false);
  let notePickerOpen = $state(false);

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

  function handleColor(color: MarkdownColorId) {
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

  function handleSlashSelect(block: SlashBlockId) {
    if (!textareaEl) return;
    if (block === "wikilink") {
      slashOpen = false;
      notePickerOpen = true;
      return;
    }
    captureSelection();
    const result = insertSlashBlock(draft, selectionStart, block);
    slashOpen = false;
    void applyEdit(result);
  }

  function handleNotePick(path: string) {
    if (!textareaEl) return;
    captureSelection();
    const label =
      vault.labelByPath().get(path) ??
      vaultDisplayTitle(path.split("/").pop()?.replace(/\.md$/i, "") ?? path, path);
    const result = insertVaultWikilink(draft, selectionStart, path, label);
    notePickerOpen = false;
    void applyEdit(result);
  }

  function closeSlashMenu() {
    slashOpen = false;
  }

  function handleKeydown(event: KeyboardEvent) {
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
    "vault-editor-textarea textarea h-full w-full resize-none rounded-none border-0 bg-surface-950 text-sm leading-relaxed";
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

  <div class="flex min-h-0 flex-1">
    {#if split && preview && onSplitResize}
      <SplitPane
        width={splitWidth}
        side="left"
        min={splitMin}
        max={splitMax}
        onResize={onSplitResize}
      >
        <textarea
          bind:this={textareaEl}
          class={textareaClass}
          bind:value={draft}
          {disabled}
          oninput={handleInput}
          onkeydown={handleKeydown}
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
      </SplitPane>
      {@render preview()}
    {:else}
      <textarea
        bind:this={textareaEl}
        class="{textareaClass} flex-1"
        bind:value={draft}
        {disabled}
        oninput={handleInput}
        onkeydown={handleKeydown}
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
    {/if}
  </div>
</div>
