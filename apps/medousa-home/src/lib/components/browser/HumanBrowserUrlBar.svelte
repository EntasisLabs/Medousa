<script lang="ts">
  import { untrack } from "svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { setMobileBrowserUrlFocus } from "$lib/utils/mobileKeyboardViewport";

  interface Props {
    urlBarFocusNonce?: number;
    /** Safari-style pill bar for mobile Web tab. */
    mobile?: boolean;
  }

  let { urlBarFocusNonce = 0, mobile = false }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);
  let blurTimer: ReturnType<typeof setTimeout> | undefined;
  let hasMounted = false;

  $effect(() => {
    // Track explicit focus requests (desktop bumps the nonce).
    urlBarFocusNonce;
    if (!hasMounted) {
      hasMounted = true;
      // On mobile, entering the Web tab must NOT pop the keyboard. Only
      // auto-focus when there is no page yet (about:blank); otherwise wait
      // for the user to tap the bar.
      if (mobile) {
        const isBlank = untrack(() => humanBrowser.activeUrl) === "about:blank";
        if (!isBlank) return;
      }
      inputEl?.focus();
      inputEl?.select();
      return;
    }
    inputEl?.focus();
    inputEl?.select();
  });

  function handleSubmit(event: Event) {
    event.preventDefault();
    const url = humanBrowser.urlDraft.trim();
    if (!url) return;
    void humanBrowser.navigate(url);
    inputEl?.blur();
  }

  function handleFocus() {
    if (!mobile) return;
    if (blurTimer) {
      clearTimeout(blurTimer);
      blurTimer = undefined;
    }
    setMobileBrowserUrlFocus(true);
    window.dispatchEvent(new CustomEvent("medousa-browser-url-focus"));
  }

  function handleBlur() {
    if (!mobile) return;
    blurTimer = setTimeout(() => {
      setMobileBrowserUrlFocus(false);
      blurTimer = undefined;
      window.dispatchEvent(new CustomEvent("medousa-browser-url-blur"));
    }, 150);
  }
</script>

<form
  class="{mobile
    ? 'mobile-browser-url-dock'
    : 'flex min-w-0 flex-1 items-center gap-2'}"
  onsubmit={handleSubmit}
>
  <input
    bind:this={inputEl}
    type="text"
    enterkeyhint="go"
    class="input min-w-0 flex-1 text-sm {mobile
      ? 'mobile-browser-url-pill rounded-full text-left'
      : ''}"
    placeholder={mobile ? "Search or enter URL" : "Search or enter URL"}
    bind:value={humanBrowser.urlDraft}
    spellcheck="false"
    autocomplete="off"
    aria-label="Address bar"
    onfocus={handleFocus}
    onblur={handleBlur}
  />
  {#if !mobile}
    <button type="submit" class="btn btn-sm variant-filled-primary shrink-0">Go</button>
  {/if}
</form>
