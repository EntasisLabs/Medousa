<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ArrowLeft, ArrowRight, RefreshCw, X } from "@lucide/svelte";
  import HumanBrowserTabBar from "$lib/components/browser/HumanBrowserTabBar.svelte";
  import HumanBrowserUrlBar from "$lib/components/browser/HumanBrowserUrlBar.svelte";
  import BrowserChromeActions from "$lib/components/browser/BrowserChromeActions.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { hideBrowser, isTauri, setBrowserWindowTitle } from "$lib/window";
  import type { HumanBrowserNavigatedPayload } from "$lib/humanBrowser";

  let urlBarFocusNonce = $state(0);

  $effect(() => {
    const title = humanBrowser.scopeLabel;
    void setBrowserWindowTitle(title === "Web" ? "Medousa Web" : title);
  });

  onMount(() => {
    settings.applyTheme();

    const unlisteners: Promise<() => void>[] = [];
    unlisteners.push(
      listen<HumanBrowserNavigatedPayload>("human-browser-navigated", (event) => {
        humanBrowser.syncFromNative(event.payload);
      }),
    );

    const onKeydown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey;
      if (!mod) return;
      const key = event.key.toLowerCase();
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);

      if (key === "l") {
        event.preventDefault();
        urlBarFocusNonce += 1;
        return;
      }
      if (typing) return;

      if (key === "t") {
        event.preventDefault();
        void humanBrowser.openTab();
        return;
      }
      if (key === "w") {
        event.preventDefault();
        const active = humanBrowser.activeTab;
        if (active) void humanBrowser.closeTab(active.id);
        return;
      }
      if (key === "r") {
        event.preventDefault();
        void humanBrowser.reload();
      }
    };
    window.addEventListener("keydown", onKeydown);

    return () => {
      window.removeEventListener("keydown", onKeydown);
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  });

  async function handleClose() {
    if (isTauri()) await hideBrowser();
  }
</script>

<!-- Fixed-height chrome strip — height must stay in sync with CHROME_HEIGHT_LOGICAL in human_browser.rs -->
<div class="human-browser-chrome relative z-50 flex h-[132px] w-full flex-col overflow-hidden bg-surface-950 text-surface-50">
  <HumanBrowserTabBar />

  <div class="flex shrink-0 items-center gap-2 border-b border-surface-800 px-2 py-1.5">
    <div class="flex shrink-0 items-center gap-1">
      <button
        type="button"
        class="btn btn-icon btn-sm"
        aria-label="Back"
        disabled={!humanBrowser.canGoBack}
        onclick={() => void humanBrowser.goBack()}
      >
        <ArrowLeft size={16} />
      </button>
      <button
        type="button"
        class="btn btn-icon btn-sm"
        aria-label="Forward"
        disabled={!humanBrowser.canGoForward}
        onclick={() => void humanBrowser.goForward()}
      >
        <ArrowRight size={16} />
      </button>
      <button
        type="button"
        class="btn btn-icon btn-sm"
        aria-label="Reload"
        onclick={() => void humanBrowser.reload()}
      >
        <RefreshCw size={16} />
      </button>
    </div>
    <HumanBrowserUrlBar {urlBarFocusNonce} />
    <BrowserChromeActions />
    {#if isTauri()}
      <button
        type="button"
        class="btn btn-icon btn-sm shrink-0"
        aria-label="Close browser"
        title="Close"
        onclick={handleClose}
      >
        <X size={16} />
      </button>
    {/if}
  </div>

  {#if humanBrowser.loading}
    <div class="h-0.5 shrink-0 bg-primary-500/80"></div>
  {/if}
</div>
