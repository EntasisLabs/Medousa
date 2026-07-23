<script lang="ts">
  import { Bookmark, Plus, RefreshCw, Search, Square } from "@lucide/svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import {
    dispatchBrowserFocusUrl,
    dispatchBrowserOpenBookmarks,
  } from "$lib/utils/browserChromeEvents";

  interface Props {
    onNavigated?: () => void;
  }

  let { onNavigated }: Props = $props();

  const loading = $derived(humanBrowser.loading);

  async function newTab() {
    await humanBrowser.openTab("about:blank");
    dispatchBrowserFocusUrl();
    onNavigated?.();
  }

  function focusUrl() {
    onNavigated?.();
    dispatchBrowserFocusUrl();
  }

  function openFind() {
    onNavigated?.();
    humanBrowser.openFindBar();
  }

  function openBookmarks() {
    onNavigated?.();
    dispatchBrowserOpenBookmarks();
  }

  async function reloadOrStop() {
    if (loading) {
      await humanBrowser.stop();
      return;
    }
    await humanBrowser.reload();
  }
</script>

<div class="lme-dock-leading-ghost min-w-0 flex-1" aria-hidden="true"></div>

<button
  type="button"
  class="vault-dock-icon-btn"
  title="New tab"
  aria-label="New tab"
  onclick={() => void newTab()}
>
  <Plus size={15} strokeWidth={1.75} />
</button>

<div class="lme-dock-chrome-secondary flex shrink-0 items-center gap-0.5">
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Focus URL"
    aria-label="Focus URL"
    onclick={focusUrl}
  >
    <span class="text-[11px] font-semibold tracking-tight">URL</span>
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title={loading ? "Stop" : "Reload"}
    aria-label={loading ? "Stop" : "Reload"}
    onclick={() => void reloadOrStop()}
  >
    {#if loading}
      <Square size={12} strokeWidth={2.25} />
    {:else}
      <RefreshCw size={15} strokeWidth={1.75} />
    {/if}
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Bookmarks"
    aria-label="Bookmarks"
    onclick={openBookmarks}
  >
    <Bookmark size={15} strokeWidth={1.75} />
  </button>
</div>

<button
  type="button"
  class="vault-dock-icon-btn"
  title="Find in page"
  aria-label="Find in page"
  onclick={openFind}
>
  <Search size={15} strokeWidth={1.75} />
</button>
