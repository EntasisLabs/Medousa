<script lang="ts">
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";

  interface Props {
    urlBarFocusNonce?: number;
  }

  let { urlBarFocusNonce = 0 }: Props = $props();

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
    class="input min-w-0 flex-1 text-sm"
    placeholder="Search or enter URL"
    bind:value={humanBrowser.urlDraft}
    spellcheck="false"
    autocomplete="off"
    aria-label="Address bar"
  />
  <button type="submit" class="btn btn-sm variant-filled-primary shrink-0">Go</button>
</form>
