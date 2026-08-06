<script lang="ts">
  import { Bookmark, Plus, RefreshCw, Search, Square } from "@lucide/svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import {
    dispatchBrowserFocusUrl,
    dispatchBrowserOpenBookmarks,
  } from "$lib/utils/browserChromeEvents";
  import { titleWithShortcut } from "$lib/utils/keyboardShortcutsCatalog";

  interface Props {
    onNavigated?: () => void;
    variant?: "popover" | "rail-row";
  }

  let { onNavigated, variant = "popover" }: Props = $props();

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

{#if variant === "popover"}
  <div class="lme-dock-leading-ghost min-w-0 flex-1" aria-hidden="true"></div>
{:else}
  <div class="min-w-0 flex-1" aria-hidden="true"></div>
{/if}

<button
  type="button"
  class="vault-dock-icon-btn"
  title={titleWithShortcut("New tab", "browser-new-tab")}
  aria-label="New tab"
  onclick={() => void newTab()}
>
  <Plus size={15} strokeWidth={1.75} />
</button>

{#if variant === "popover"}
  <div class="lme-dock-chrome-secondary flex shrink-0 items-center gap-0.5">
    <button
      type="button"
      class="vault-dock-icon-btn"
      title={titleWithShortcut("Focus URL", "browser-focus-url")}
      aria-label="Focus URL"
      onclick={focusUrl}
    >
      <span class="text-[11px] font-semibold tracking-tight">URL</span>
    </button>
    <button
      type="button"
      class="vault-dock-icon-btn"
      title={loading ? "Stop" : titleWithShortcut("Reload", "browser-reload")}
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
      title={titleWithShortcut("Bookmarks", "browser-bookmarks")}
      aria-label="Bookmarks"
      onclick={openBookmarks}
    >
      <Bookmark size={15} strokeWidth={1.75} />
    </button>
  </div>
{/if}

<button
  type="button"
  class="vault-dock-icon-btn"
  title={titleWithShortcut("Find in page", "browser-find")}
  aria-label="Find in page"
  onclick={openFind}
>
  <Search size={15} strokeWidth={1.75} />
</button>
