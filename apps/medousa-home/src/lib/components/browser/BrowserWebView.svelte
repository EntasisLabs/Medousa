<script lang="ts">
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";

  interface Props {
    visible?: boolean;
    url: string;
  }

  let { visible = true, url }: Props = $props();

  let iframeEl = $state<HTMLIFrameElement | null>(null);

  export async function reload() {
    if (iframeEl) iframeEl.src = url;
    else await humanBrowser.reload();
  }

  export async function goBack() {
    await humanBrowser.goBack();
  }

  export async function goForward() {
    await humanBrowser.goForward();
  }
</script>

<div class="h-full min-h-0 w-full">
  {#if visible && url && url !== "about:blank"}
    <iframe
      bind:this={iframeEl}
      title="Web browser"
      src={url}
      class="block h-full w-full border-0 bg-white"
      sandbox="allow-scripts allow-forms allow-same-origin allow-popups allow-popups-to-escape-sandbox"
    ></iframe>
  {/if}
</div>
