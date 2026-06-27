<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ArrowLeft, ArrowRight, BookmarkPlus, Globe, RefreshCw } from "@lucide/svelte";
  import HumanBrowserTabBar from "$lib/components/browser/HumanBrowserTabBar.svelte";
  import HumanBrowserUrlBar from "$lib/components/browser/HumanBrowserUrlBar.svelte";
  import {
    humanBrowserEmbedApplyLayout,
    humanBrowserEmbedHide,
    type HumanBrowserNavigatedPayload,
  } from "$lib/humanBrowser";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { isTauri } from "$lib/platform";
  import { layoutDesktopRails } from "$lib/utils/desktopRails";

  interface Props {
    visible?: boolean;
    workRailVisible?: boolean;
  }

  let { visible = true, workRailVisible = false }: Props = $props();

  let urlBarFocusNonce = $state(0);
  let saving = $state(false);

  async function presentEmbed() {
    if (!isTauri() || !visible) return;
    const rails = layoutDesktopRails({
      viewportWidth: layout.viewportWidth,
      activityCollapsed: layout.activityCollapsed,
      activityWidth: layout.activityWidth,
      workInspectorOpen: false,
      workInspectorWidth: layout.workInspectorWidth,
    });
    await humanBrowserEmbedApplyLayout({
      activityWidth: rails.activityPaneWidth,
      activityCollapsed: layout.activityCollapsed,
      workRailVisible,
    });
  }

  $effect(() => {
    if (!isTauri() || !visible) return;
    layout.activityWidth;
    layout.activityCollapsed;
    layout.viewportWidth;
    workRailVisible;
    void presentEmbed();
    return () => {
      void humanBrowserEmbedHide();
    };
  });

  onMount(() => {
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

  async function saveToVault() {
    const url = humanBrowser.activeUrl;
    if (!url || url === "about:blank" || saving) return;
    saving = true;
    try {
      const title = humanBrowser.activeTab?.title?.trim() || url;
      const content = `# ${title}\n\nSource: ${url}\n`;
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

<div class="flex h-full min-h-0 flex-col bg-surface-950 text-surface-50">
  <!-- Fixed-height chrome — must stay in sync with CHROME_HEIGHT_LOGICAL in human_browser.rs -->
  <div class="flex h-[132px] w-full shrink-0 flex-col overflow-hidden">
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
      <button
        type="button"
        class="btn btn-sm variant-soft-surface shrink-0"
        disabled={saving || humanBrowser.activeUrl === "about:blank"}
        onclick={() => void saveToVault()}
        title="Save page to Library"
      >
        <BookmarkPlus size={14} class="mr-1 inline" />
        Save
      </button>
    </div>

    {#if humanBrowser.loading}
      <div class="h-0.5 shrink-0 bg-primary-500/80"></div>
    {/if}
  </div>

  <!-- Placeholder beneath the native child webview (Rust-positioned, not DOM-synced). -->
  <div class="relative min-h-0 flex-1 overflow-hidden bg-surface-900">
    {#if humanBrowser.activeUrl === "about:blank"}
      <div
        class="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 text-surface-400"
      >
        <Globe size={40} strokeWidth={1.25} class="opacity-40" />
        <p class="text-sm">Enter a URL above or open a link from Chat</p>
      </div>
    {/if}
  </div>
</div>
