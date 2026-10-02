<script lang="ts">
  import "$lib/styles/vault-find.postcss";
  import "$lib/styles/editor-find.postcss";
  import { onMount } from "svelte";
  import { CaseSensitive, ChevronDown, ChevronUp, Replace, X } from "@lucide/svelte";
  import { titleWithKeys } from "$lib/utils/keyboardShortcutsCatalog";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import { VAULT_FIND_INPUT_ID } from "$lib/utils/vaultFindInNote";

  let inputEl = $state<HTMLInputElement | null>(null);
  let replaceInputEl = $state<HTMLInputElement | null>(null);

  const showStatus = $derived(Boolean(vaultFind.query.trim()));

  onMount(() => {
    inputEl?.focus();
    inputEl?.select();
  });

  $effect(() => {
    if (vaultFind.open && vaultFind.replaceMode) {
      queueMicrotask(() => replaceInputEl?.focus());
    }
  });

  function handleInput(event: Event) {
    vaultFind.setQuery((event.target as HTMLInputElement).value);
  }

  function handleReplaceInput(event: Event) {
    vaultFind.setReplaceQuery((event.target as HTMLInputElement).value);
  }

  function handleNext() {
    vaultFind.next();
  }

  function handlePrev() {
    vaultFind.prev();
  }

  function keepFocus(event: MouseEvent) {
    event.preventDefault();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      vaultFind.close();
      return;
    }
    if (
      (event.metaKey || event.ctrlKey) &&
      event.altKey &&
      event.key.toLowerCase() === "f"
    ) {
      event.preventDefault();
      vaultFind.replaceMode = true;
      queueMicrotask(() => replaceInputEl?.focus());
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.shiftKey) handlePrev();
      else handleNext();
    }
  }

  function handleReplaceKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      vaultFind.close();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.metaKey || event.ctrlKey) {
        vaultFind.replaceAll();
      } else {
        vaultFind.replaceOne();
      }
    }
  }
</script>

<div
  class="editor-find-bar"
  class:editor-find-bar--replace={vaultFind.replaceMode}
  role="search"
  aria-label="Find in note"
>
  <div class="editor-find-bar-row">
    <label class="sr-only" for={VAULT_FIND_INPUT_ID}>Find in note</label>
    <input
      id={VAULT_FIND_INPUT_ID}
      bind:this={inputEl}
      class="editor-find-input"
      type="text"
      placeholder="Find"
      value={vaultFind.query}
      oninput={handleInput}
      onkeydown={handleKeydown}
      autocomplete="off"
      spellcheck="false"
    />
    {#if showStatus}
      <span class="editor-find-divider" aria-hidden="true"></span>
      <span class="editor-find-status" aria-live="polite">{vaultFind.statusLabel}</span>
    {/if}
    <span class="editor-find-divider" aria-hidden="true"></span>
    <button
      type="button"
      class="editor-find-btn"
      class:editor-find-btn--active={vaultFind.matchCase}
      aria-label="Match case"
      aria-pressed={vaultFind.matchCase}
      title="Match case"
      onmousedown={keepFocus}
      onclick={() => vaultFind.toggleMatchCase()}
    >
      <CaseSensitive size={13} strokeWidth={2.25} />
    </button>
    <button
      type="button"
      class="editor-find-btn"
      class:editor-find-btn--active={vaultFind.replaceMode}
      aria-label="Toggle replace"
      aria-pressed={vaultFind.replaceMode}
      title={titleWithKeys("Replace", "mod:⌥F")}
      onmousedown={keepFocus}
      onclick={() => {
        vaultFind.replaceMode = !vaultFind.replaceMode;
        if (vaultFind.replaceMode) queueMicrotask(() => replaceInputEl?.focus());
      }}
    >
      <Replace size={13} strokeWidth={2.25} />
    </button>
    <div class="editor-find-nav">
      <button
        type="button"
        class="editor-find-btn"
        aria-label="Previous match"
        title="Previous match (Shift+Enter)"
        disabled={vaultFind.matchCount === 0}
        onmousedown={keepFocus}
        onclick={handlePrev}
      >
        <ChevronUp size={13} strokeWidth={2.25} />
      </button>
      <button
        type="button"
        class="editor-find-btn"
        aria-label="Next match"
        title="Next match (Enter)"
        disabled={vaultFind.matchCount === 0}
        onmousedown={keepFocus}
        onclick={handleNext}
      >
        <ChevronDown size={13} strokeWidth={2.25} />
      </button>
    </div>
    <span class="editor-find-divider" aria-hidden="true"></span>
    <button
      type="button"
      class="editor-find-btn"
      aria-label="Close find"
      title="Close (Esc)"
      onmousedown={keepFocus}
      onclick={() => vaultFind.close()}
    >
      <X size={13} strokeWidth={2.25} />
    </button>
  </div>
  {#if vaultFind.replaceMode}
    <div class="editor-find-bar-row editor-find-bar-row--replace">
      <label class="sr-only" for="vault-find-replace-input">Replace with</label>
      <input
        id="vault-find-replace-input"
        bind:this={replaceInputEl}
        class="editor-find-input"
        type="text"
        placeholder="Replace"
        value={vaultFind.replaceQuery}
        oninput={handleReplaceInput}
        onkeydown={handleReplaceKeydown}
        autocomplete="off"
        spellcheck="false"
      />
      <button
        type="button"
        class="editor-find-text-btn"
        disabled={vaultFind.matchCount === 0}
        onmousedown={keepFocus}
        onclick={() => vaultFind.replaceOne()}
      >
        Replace
      </button>
      <button
        type="button"
        class="editor-find-text-btn"
        disabled={vaultFind.matchCount === 0}
        onmousedown={keepFocus}
        onclick={() => vaultFind.replaceAll()}
      >
        All
      </button>
    </div>
  {/if}
</div>
