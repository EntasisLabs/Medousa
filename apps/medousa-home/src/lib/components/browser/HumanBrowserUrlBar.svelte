<script lang="ts">
  import { untrack } from "svelte";
  import { humanBrowserForWindow } from "$lib/stores/humanBrowserSurface";
  import { browserHistory } from "$lib/stores/browserHistory.svelte";
  import { hostnameFromUrl, tabDisplayLabel } from "$lib/utils/browserFavicon";
  import { setMobileBrowserUrlFocus } from "$lib/utils/mobileKeyboardViewport";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import { formatShortcut } from "$lib/platform";

  const humanBrowser = $derived(humanBrowserForWindow());

  interface Props {
    urlBarFocusNonce?: number;
    /** Safari-style pill bar for mobile Web tab. */
    mobile?: boolean;
  }

  let { urlBarFocusNonce = 0, mobile = false }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);
  let blurTimer: ReturnType<typeof setTimeout> | undefined;
  let lastFocusNonce: number | null = null;
  let suggestionsOpen = $state(false);
  let suppressSuggestions = $state(false);

  const suggestions = $derived(
    suppressSuggestions ? [] : browserHistory.search(humanBrowser.urlDraft, 6),
  );
  const showSuggestions = $derived(
    !mobile && suggestionsOpen && suggestions.length > 0 && !suppressSuggestions,
  );

  // Native WKWebView paints above DOM — hide embed while the suggestion list is open.
  $effect(() => {
    if (!showSuggestions) return;
    void pushBrowserPopoverOverlay();
    return () => {
      void popBrowserPopoverOverlay();
    };
  });

  // Only focus on explicit request (⌘L / focus event). Do not steal focus when the
  // browser pane remounts or becomes the active shell tab.
  $effect(() => {
    const nonce = urlBarFocusNonce;
    if (lastFocusNonce === null) {
      lastFocusNonce = nonce;
      // Mobile blank start page: allow typing a URL without an extra tap.
      if (mobile && untrack(() => humanBrowser.activeUrl) === "about:blank") {
        inputEl?.focus();
        inputEl?.select();
      }
      return;
    }
    if (nonce === lastFocusNonce) return;
    lastFocusNonce = nonce;
    inputEl?.focus();
    inputEl?.select();
  });

  function dismissSuggestions() {
    suggestionsOpen = false;
    suppressSuggestions = true;
  }

  function handleSubmit(event: Event) {
    event.preventDefault();
    const url = humanBrowser.urlDraft.trim();
    if (!url) return;
    dismissSuggestions();
    inputEl?.blur();
    void humanBrowser.navigate(url);
  }

  function pickSuggestion(url: string) {
    humanBrowser.urlDraft = url;
    dismissSuggestions();
    inputEl?.blur();
    void humanBrowser.navigate(url);
  }

  function handleFocus() {
    if (!mobile) {
      suppressSuggestions = false;
      suggestionsOpen = true;
      return;
    }
    if (blurTimer) {
      clearTimeout(blurTimer);
      blurTimer = undefined;
    }
    setMobileBrowserUrlFocus(true);
    window.dispatchEvent(new CustomEvent("medousa-browser-url-focus"));
  }

  function handleBlur() {
    if (!mobile) {
      // Delay so suggestion mousedown can fire first.
      blurTimer = setTimeout(() => {
        suggestionsOpen = false;
        blurTimer = undefined;
      }, 150);
      return;
    }
    blurTimer = setTimeout(() => {
      setMobileBrowserUrlFocus(false);
      blurTimer = undefined;
      window.dispatchEvent(new CustomEvent("medousa-browser-url-blur"));
    }, 150);
  }

  function handleInput() {
    suppressSuggestions = false;
    suggestionsOpen = true;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      dismissSuggestions();
      inputEl?.blur();
    }
  }
</script>

<form
  class="{mobile
    ? 'mobile-browser-url-dock'
    : 'browser-url-bar flex min-w-0 flex-1 items-center'} relative"
  onsubmit={handleSubmit}
>
  <div
    class="flex min-w-0 flex-1 items-center"
    role="combobox"
    aria-expanded={showSuggestions}
    aria-haspopup="listbox"
    aria-controls="browser-url-suggestions"
  >
  <input
    bind:this={inputEl}
    type="text"
    enterkeyhint="go"
    class="min-w-0 flex-1 text-sm {mobile
      ? 'mobile-browser-url-pill rounded-full text-left'
      : 'browser-url-bar-input'}"
    placeholder="Search or enter URL"
    bind:value={humanBrowser.urlDraft}
    spellcheck="false"
    autocomplete="off"
    autocorrect="off"
    autocapitalize="off"
    aria-label="Address bar"
    aria-autocomplete="list"
    title="Search or enter URL ({formatShortcut('L')})"
    onfocus={handleFocus}
    onblur={handleBlur}
    oninput={handleInput}
    onkeydown={handleKeydown}
  />
  {#if showSuggestions}
    <ul id="browser-url-suggestions" class="browser-url-suggestions" role="listbox">
      {#each suggestions as entry (entry.url + entry.visitedAt)}
        <li>
          <button
            type="button"
            class="browser-url-suggestion"
            role="option"
            aria-selected={false}
            onmousedown={(event) => {
              event.preventDefault();
              pickSuggestion(entry.url);
            }}
          >
            <span class="browser-url-suggestion-title">
              {tabDisplayLabel(entry.title, entry.url)}
            </span>
            <span class="browser-url-suggestion-host">{hostnameFromUrl(entry.url)}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  </div>
</form>
