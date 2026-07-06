<script lang="ts">
  import { ChevronDown, ChevronUp, X } from "@lucide/svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";

  let query = $state("");
  let lastFound = $state<boolean | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (humanBrowser.findOpen) {
      query = "";
      lastFound = null;
      void Promise.resolve().then(() => inputEl?.focus());
    }
  });

  async function find(forward: boolean) {
    const trimmed = query.trim();
    if (!trimmed) return;
    const result = await humanBrowser.findInPage(trimmed, forward);
    lastFound = result.found;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!humanBrowser.findOpen) return;

    if (event.key === "Escape") {
      event.preventDefault();
      humanBrowser.closeFindBar();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      void find(!event.shiftKey);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if humanBrowser.findOpen}
  <div class="browser-find-bar" role="search">
    <input
      bind:this={inputEl}
      class="browser-find-input"
      type="search"
      placeholder="Find in page"
      bind:value={query}
    />
    <button
      type="button"
      class="btn btn-icon btn-sm"
      aria-label="Previous match"
      onclick={() => void find(false)}
    >
      <ChevronUp size={16} />
    </button>
    <button
      type="button"
      class="btn btn-icon btn-sm"
      aria-label="Next match"
      onclick={() => void find(true)}
    >
      <ChevronDown size={16} />
    </button>
    {#if lastFound === false}
      <span class="browser-find-status">No match</span>
    {/if}
    <button
      type="button"
      class="btn btn-icon btn-sm"
      aria-label="Close find"
      onclick={() => humanBrowser.closeFindBar()}
    >
      <X size={16} />
    </button>
  </div>
{/if}
