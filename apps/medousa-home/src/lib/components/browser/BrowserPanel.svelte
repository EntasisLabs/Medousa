<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowLeft, ArrowRight, Globe, RefreshCw } from "@lucide/svelte";
  import BrowserTabBar from "$lib/components/browser/BrowserTabBar.svelte";
  import BrowserUrlBar from "$lib/components/browser/BrowserUrlBar.svelte";
  import BrowserWebView from "$lib/components/browser/BrowserWebView.svelte";
  import { browser } from "$lib/stores/browser.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { bridgeSnapshot } from "$lib/browserBridge";

  interface Props {
    visible?: boolean;
    mobile?: boolean;
  }

  let { visible = true, mobile = false }: Props = $props();

  const boundsSyncKey = $derived(`${layout.viewportWidth}`);

  let browserPaneEl = $state<HTMLDivElement | null>(null);
  let webView = $state<{ reload: () => Promise<void>; goBack: () => Promise<void>; goForward: () => Promise<void> } | null>(null);
  let saving = $state(false);

  onMount(() => {
    void browser.ensureTabGroup(chat.sessionId);
    void browser.refreshFromBridge();
  });

  $effect(() => {
    if (!visible) return;
    void browser.ensureTabGroup(chat.sessionId);
  });

  async function reloadView() {
    await webView?.reload();
  }

  async function saveToVault() {
    const url = browser.activeUrl;
    if (!url || url === "about:blank" || saving) return;
    saving = true;
    try {
      let excerpt = "";
      if (browser.tabGroupId) {
        const snapshot = await bridgeSnapshot(browser.tabGroupId, 2000);
        excerpt = snapshot?.markdown?.trim() ?? "";
      }
      const title = browser.activeTab?.title?.trim() || url;
      const content = `# ${title}\n\nSource: ${url}\n\n${excerpt ? `${excerpt}\n` : ""}`;
      await vault.createNote({
        spaceId: vault.activeSpace?.id ?? "other",
        title,
        content,
      });
    } finally {
      saving = false;
    }
  }
</script>

{#if visible}
  <div class="flex h-full min-h-0 flex-col bg-surface-950">
    {#if !mobile}
      <BrowserTabBar />
    {/if}
    <div
      class="flex shrink-0 items-center gap-2 border-b border-surface-800 px-3 {mobile
        ? 'py-1.5'
        : 'py-2'}"
    >
      <div class="flex shrink-0 items-center gap-1">
        <button
          type="button"
          class="btn btn-icon btn-sm"
          aria-label="Back"
          disabled={!browser.canGoBack}
          onclick={() => void webView?.goBack()}
        >
          <ArrowLeft size={16} />
        </button>
        <button
          type="button"
          class="btn btn-icon btn-sm"
          aria-label="Forward"
          disabled={!browser.canGoForward}
          onclick={() => void webView?.goForward()}
        >
          <ArrowRight size={16} />
        </button>
        <button type="button" class="btn btn-icon btn-sm" aria-label="Reload" onclick={() => void reloadView()}>
          <RefreshCw size={16} />
        </button>
      </div>
      <BrowserUrlBar />
    </div>
    <div
      bind:this={browserPaneEl}
      data-browser-surface
      class="relative min-h-0 flex-1 overflow-hidden bg-surface-950"
    >
      {#if browser.activeUrl && browser.activeUrl !== "about:blank"}
        <BrowserWebView
          bind:this={webView}
          {visible}
          url={browser.activeUrl}
          measureEl={browserPaneEl}
          {boundsSyncKey}
        />
      {:else}
        <div class="flex h-full min-h-0 flex-col items-center justify-center gap-3 bg-surface-900 text-surface-300">
          <Globe size={40} strokeWidth={1.25} />
          <p class="text-sm">Enter a URL above to start browsing.</p>
        </div>
      {/if}
      {#if browser.loading}
        <div class="pointer-events-none absolute inset-x-0 top-0 h-0.5 bg-primary-500/80"></div>
      {/if}
    </div>
  </div>
{/if}
