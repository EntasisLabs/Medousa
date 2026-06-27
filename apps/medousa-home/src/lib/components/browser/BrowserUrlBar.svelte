<script lang="ts">
  import { browser } from "$lib/stores/browser.svelte";

  interface Props {
    onSubmit?: (url: string) => void;
  }

  let { onSubmit }: Props = $props();

  function handleSubmit(event: Event) {
    event.preventDefault();
    const url = browser.urlDraft.trim();
    if (!url) return;
    onSubmit?.(url);
    void browser.navigate(url, "user");
  }
</script>

<form class="flex min-w-0 flex-1 items-center gap-2" onsubmit={handleSubmit}>
  <input
    type="text"
    class="input min-w-0 flex-1 text-sm"
    placeholder="Search or enter URL"
    bind:value={browser.urlDraft}
    spellcheck="false"
    autocomplete="off"
    aria-label="Address bar"
  />
  <button type="submit" class="btn btn-sm variant-filled-primary shrink-0">Go</button>
</form>
