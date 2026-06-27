<script lang="ts">
  import { onMount } from "svelte";
  import { browser } from "$lib/stores/browser.svelte";

  interface Props {
    visible?: boolean;
    url: string;
    measureEl?: HTMLElement | null;
    boundsSyncKey?: string;
  }

  let { visible = true, url }: Props = $props();

  let iframeEl = $state<HTMLIFrameElement | null>(null);

  onMount(() => {
    return () => {};
  });

  export async function reload() {
    if (iframeEl) iframeEl.src = url;
  }

  export async function goBack() {
    browser.goBack();
  }

  export async function goForward() {
    browser.goForward();
  }
</script>

<div class="h-full min-h-0 w-full">
  {#if url && url !== "about:blank"}
    <iframe
      bind:this={iframeEl}
      title="Web browser"
      src={url}
      class="block h-full w-full border-0 bg-white"
      sandbox="allow-scripts allow-forms allow-same-origin allow-popups allow-popups-to-escape-sandbox"
    ></iframe>
  {/if}
</div>
