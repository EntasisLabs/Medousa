<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ArrowLeft, ArrowRight, Globe, RefreshCw } from "@lucide/svelte";
  import HumanBrowserTabBar from "$lib/components/browser/HumanBrowserTabBar.svelte";
  import HumanBrowserUrlBar from "$lib/components/browser/HumanBrowserUrlBar.svelte";
  import BrowserChromeActions from "$lib/components/browser/BrowserChromeActions.svelte";
  import {
    humanBrowserEmbedApplyLayout,
    humanBrowserEmbedHide,
    humanBrowserEmbedShow,
    humanBrowserSetMobileShellActive,
    type HumanBrowserNavigatedPayload,
  } from "$lib/humanBrowser";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { isTauri, shouldUseMobileShell } from "$lib/platform";
  import { layoutDesktopRails } from "$lib/utils/desktopRails";

  interface Props {
    visible?: boolean;
    workRailVisible?: boolean;
  }

  let { visible = true, workRailVisible = false }: Props = $props();

  let urlBarFocusNonce = $state(0);
  let embedGeneration = 0;

  async function presentEmbed() {
    if (!isTauri() || !visible || layout.isMobile || shouldUseMobileShell()) return;
    const gen = ++embedGeneration;
    await humanBrowserSetMobileShellActive(false);
    if (gen !== embedGeneration) return;
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
    if (gen !== embedGeneration) return;
  }

  $effect(() => {
    if (!isTauri() || !visible || layout.isMobile) return;
    layout.activityWidth;
    layout.activityCollapsed;
    layout.viewportWidth;
    workRailVisible;
    void presentEmbed();
    return () => {
      if (shouldUseMobileShell()) return;
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

    const onResize = () => {
      void presentEmbed();
    };
    window.addEventListener("resize", onResize);

    return () => {
      window.removeEventListener("keydown", onKeydown);
      window.removeEventListener("resize", onResize);
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  });
</script>

<!-- Native webview fills panel; chrome floats on top (Safari-style). -->
<div class="relative h-full min-h-0 bg-surface-950 text-surface-50" data-browser-panel>
  <div class="absolute inset-0 overflow-hidden bg-surface-900" data-browser-embed-host>
    {#if humanBrowser.activeUrl === "about:blank"}
      <div
        class="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 pt-28 text-surface-400"
      >
        <Globe size={40} strokeWidth={1.25} class="opacity-40" />
        <p class="text-sm">Enter a URL above or open a link from Chat</p>
      </div>
    {/if}
    {#if humanBrowser.loading}
      <div class="pointer-events-none absolute inset-x-0 top-28 h-0.5 bg-primary-500/80"></div>
    {/if}
  </div>

  <div class="human-browser-chrome-overlay pointer-events-none absolute inset-x-0 top-0 z-50">
    <div class="pointer-events-auto">
      <HumanBrowserTabBar />

      <div class="flex shrink-0 items-center gap-2 border-b border-surface-800/80 px-2 py-1.5">
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
      </div>

      {#if humanBrowser.loading}
        <div class="h-0.5 shrink-0 bg-primary-500/80"></div>
      {/if}
    </div>
  </div>
</div>
