<script lang="ts">
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";

  interface Props {
    urlBarFocusNonce?: number;
    /** Safari-style pill bar for mobile Web tab. */
    mobile?: boolean;
  }

  let { urlBarFocusNonce = 0, mobile = false }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);

  $effect(() => {
    urlBarFocusNonce;
    inputEl?.focus();
    inputEl?.select();
  });

  function handleSubmit(event: Event) {
    event.preventDefault();
    const url = humanBrowser.urlDraft.trim();
    if (!url) return;
    void humanBrowser.navigate(url);
  }
</script>

<form class="flex min-w-0 flex-1 items-center gap-2" onsubmit={handleSubmit}>
  <input
    bind:this={inputEl}
    type="text"
    class="input min-w-0 flex-1 text-sm {mobile
      ? 'mobile-browser-url-pill rounded-full border-surface-700 bg-surface-800/90 py-1.5 text-center'
      : ''}"
    placeholder={mobile ? "Search or enter URL" : "Search or enter URL"}
    bind:value={humanBrowser.urlDraft}
    spellcheck="false"
    autocomplete="off"
    aria-label="Address bar"
  />
  {#if !mobile}
    <button type="submit" class="btn btn-sm variant-filled-primary shrink-0">Go</button>
  {/if}
</form>
