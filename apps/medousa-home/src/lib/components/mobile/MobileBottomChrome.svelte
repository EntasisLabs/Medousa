<script lang="ts">
  import MobileChatComposer from "$lib/components/mobile/MobileChatComposer.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { attachMobileBottomChromeLayout } from "$lib/utils/mobileKeyboardViewport";

  let chromeEl: HTMLElement | undefined = $state();

  $effect(() => {
    if (!chromeEl) return;
    return attachMobileBottomChromeLayout(chromeEl);
  });

  // Keep the chrome node mounted so --mobile-bottom-chrome-height stays in sync.
  // Tab bar is gone; non-chat tabs collapse to zero height.
  const showComposer = $derived(layout.mobileTab === "chat");
</script>

<div
  bind:this={chromeEl}
  class="mobile-bottom-chrome"
  class:mobile-bottom-chrome-collapsed={!showComposer}
  data-show-composer={showComposer ? "true" : "false"}
  data-hide-tabs="true"
  aria-hidden={!showComposer}
>
  {#if showComposer}
    <MobileChatComposer />
  {/if}
</div>
