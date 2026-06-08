<script lang="ts">
  import MobileChatComposer from "$lib/components/mobile/MobileChatComposer.svelte";
  import MobileTabBar from "$lib/components/mobile/MobileTabBar.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { attachMobileBottomChromeLayout } from "$lib/utils/mobileKeyboardViewport";

  let chromeEl: HTMLElement | undefined = $state();

  $effect(() => {
    if (!chromeEl) return;
    return attachMobileBottomChromeLayout(chromeEl);
  });
</script>

<div
  bind:this={chromeEl}
  class="mobile-bottom-chrome"
  data-show-composer={layout.mobileTab === "chat" ? "true" : "false"}
>
  {#if layout.mobileTab === "chat"}
    <MobileChatComposer />
  {/if}
  <MobileTabBar />
</div>
