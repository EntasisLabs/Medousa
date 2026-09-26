<script lang="ts">
  import MobileChatComposer from "$lib/components/mobile/MobileChatComposer.svelte";
  import MedousaLiveBar from "$lib/components/mobile/MedousaLiveBar.svelte";
  import { isTauriIos } from "$lib/platform";
  import { liveVoiceState } from "$lib/liveVoice";
  import { layout } from "$lib/runtime/layout.svelte";
  import { attachMobileBottomChromeLayout } from "$lib/utils/mobileKeyboardViewport";

  let chromeEl: HTMLElement | undefined = $state();

  $effect(() => {
    if (!chromeEl) return;
    return attachMobileBottomChromeLayout(chromeEl);
  });

  // Keep the chrome node mounted so --mobile-bottom-chrome-height stays in sync.
  // Tab bar is gone; non-chat tabs collapse to zero height.
  const showComposer = $derived(layout.mobileTab === "chat");
  const showLive = $derived(isTauriIos() && ($liveVoiceState.active ||
    $liveVoiceState.phase === "connecting" || $liveVoiceState.phase === "failed"));
</script>

<div
  bind:this={chromeEl}
  class="mobile-bottom-chrome"
  class:mobile-bottom-chrome-collapsed={!showComposer && !showLive}
  data-show-composer={showComposer ? "true" : "false"}
  data-hide-tabs="true"
  aria-hidden={!showComposer && !showLive}
>
  {#if isTauriIos()}<MedousaLiveBar />{/if}
  {#if showComposer}
    <MobileChatComposer />
  {/if}
</div>
