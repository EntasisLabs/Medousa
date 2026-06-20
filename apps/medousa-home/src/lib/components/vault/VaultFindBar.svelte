<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronDown, ChevronUp, X } from "@lucide/svelte";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import { VAULT_FIND_INPUT_ID } from "$lib/utils/vaultFindInNote";

  let inputEl = $state<HTMLInputElement | null>(null);

  const showStatus = $derived(Boolean(vaultFind.query.trim()));

  onMount(() => {
    inputEl?.focus();
    inputEl?.select();
  });

  function handleInput(event: Event) {
    vaultFind.setQuery((event.target as HTMLInputElement).value);
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
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.shiftKey) handlePrev();
      else handleNext();
    }
  }
</script>

<div class="vault-find-bar" role="search" aria-label="Find in note">
  <label class="sr-only" for={VAULT_FIND_INPUT_ID}>Find in note</label>
  <input
    id={VAULT_FIND_INPUT_ID}
    bind:this={inputEl}
    class="vault-find-input"
    type="text"
    placeholder="Find"
    value={vaultFind.query}
    oninput={handleInput}
    onkeydown={handleKeydown}
    autocomplete="off"
    spellcheck="false"
  />
  {#if showStatus}
    <span class="vault-find-divider" aria-hidden="true"></span>
    <span class="vault-find-status" aria-live="polite">{vaultFind.statusLabel}</span>
  {/if}
  <span class="vault-find-divider" aria-hidden="true"></span>
  <div class="vault-find-nav">
    <button
      type="button"
      class="vault-find-btn"
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
      class="vault-find-btn"
      aria-label="Next match"
      title="Next match (Enter)"
      disabled={vaultFind.matchCount === 0}
      onmousedown={keepFocus}
      onclick={handleNext}
    >
      <ChevronDown size={13} strokeWidth={2.25} />
    </button>
  </div>
  <span class="vault-find-divider" aria-hidden="true"></span>
  <button
    type="button"
    class="vault-find-btn"
    aria-label="Close find"
    title="Close (Esc)"
    onmousedown={keepFocus}
    onclick={() => vaultFind.close()}
  >
    <X size={13} strokeWidth={2.25} />
  </button>
</div>
