<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    Bot,
    LoaderCircle,
    RefreshCw,
    UserRound,
  } from "@lucide/svelte";
  import BrowserSurfacePicker from "$lib/components/browser/BrowserSurfacePicker.svelte";
  import IsolatedBrowserViewport from "$lib/components/browser/IsolatedBrowserViewport.svelte";
  import { governedBrowser } from "$lib/stores/governedBrowser.svelte";

  interface Props {
    mobile?: boolean;
    visible?: boolean;
    shellTabChrome?: boolean;
    urlBarFocusNonce?: number;
  }

  let {
    mobile = false,
    visible = true,
    shellTabChrome = false,
    urlBarFocusNonce = 0,
  }: Props = $props();
  let urlInput = $state<HTMLInputElement | null>(null);
  let lastFocusNonce: number | null = null;

  const world = $derived(governedBrowser.selectedWorld);
  const agentDriving = $derived(world?.control === "agent");
  const awaitingOperator = $derived(world?.control === "awaiting_operator");

  $effect(() => {
    const nonce = urlBarFocusNonce;
    if (lastFocusNonce === null) {
      lastFocusNonce = nonce;
      return;
    }
    if (nonce === lastFocusNonce) return;
    lastFocusNonce = nonce;
    urlInput?.focus();
    urlInput?.select();
  });

  function navigate(event: SubmitEvent) {
    event.preventDefault();
    const destination = governedBrowser.urlDraft.trim();
    if (!destination) return;
    urlInput?.blur();
    void governedBrowser.navigate(destination);
  }
</script>

<div
  data-browser-panel
  data-governed-browser-panel
  class="human-browser-panel governed-browser-panel flex h-full min-h-0 min-w-0 w-full flex-col"
>
  {#if !mobile}
    <div class="human-browser-chrome relative z-50 flex w-full shrink-0 flex-col">
      {#if !shellTabChrome}
        <div class="governed-browser-tab-strip">
          <Bot size={14} />
          <span class="min-w-0 flex-1 truncate">{governedBrowser.activeTitle}</span>
          <span class="governed-browser-profile">
            {world?.profile.kind === "persistent" ? "Saved identity" : "Private"}
          </span>
        </div>
      {/if}

      <div class="governed-browser-handoff" role="status">
        <span class:governed-browser-status-agent={agentDriving} class:governed-browser-status-attention={awaitingOperator} class="governed-browser-status-dot"></span>
        <span class="min-w-0 flex-1 truncate">
          {agentDriving
            ? "Medousa is browsing on the workshop"
            : awaitingOperator
              ? "This browser needs you"
              : "You are browsing on the workshop"}
        </span>
        {#if agentDriving || awaitingOperator}
          <button type="button" class="btn btn-xs variant-soft-primary" onclick={() => void governedBrowser.takeControl()}>
            Take control
          </button>
        {:else}
          <button type="button" class="btn btn-xs variant-soft-surface" onclick={() => void governedBrowser.handBackToAgent()}>
            Hand back
          </button>
        {/if}
      </div>

      <div class="browser-toolbar">
        <div class="browser-nav-cluster">
          <button type="button" class="browser-nav-btn" aria-label="Back" disabled={!governedBrowser.canGoBack} onclick={() => void governedBrowser.goBack()}>
            <ArrowLeft size={15} strokeWidth={1.75} />
          </button>
          <button type="button" class="browser-nav-btn" aria-label="Forward" disabled={!governedBrowser.canGoForward} onclick={() => void governedBrowser.goForward()}>
            <ArrowRight size={15} strokeWidth={1.75} />
          </button>
          <button type="button" class="browser-nav-btn" aria-label="Reload" disabled={governedBrowser.activeUrl === "about:blank"} onclick={() => void governedBrowser.reload()}>
            <RefreshCw size={14} strokeWidth={1.75} />
          </button>
        </div>
        <form class="browser-url-bar flex min-w-0 flex-1 items-center" onsubmit={navigate}>
          <input
            bind:this={urlInput}
            class="browser-url-bar-input"
            type="text"
            enterkeyhint="go"
            aria-label="Workshop browser address"
            placeholder="Search or enter URL"
            bind:value={governedBrowser.urlDraft}
            spellcheck="false"
            autocomplete="off"
            autocorrect="off"
            autocapitalize="off"
          />
        </form>
        <BrowserSurfacePicker />
      </div>
      {#if governedBrowser.frameLoading && !governedBrowser.frameDataUrl}
        <div class="browser-loading-bar"></div>
      {/if}
    </div>
  {/if}

  <IsolatedBrowserViewport {mobile} {visible} />

  {#if governedBrowser.error}
    <div class:governed-browser-error-mobile={mobile} class="governed-browser-error" role="alert">
      <span>{governedBrowser.error}</span>
      <button type="button" onclick={() => (governedBrowser.error = null)}>Dismiss</button>
    </div>
  {/if}

  {#if mobile}
    <div data-browser-bottom-chrome class="mobile-browser-bottom-chrome governed-browser-mobile-chrome">
      <div class="governed-browser-mobile-state" role="status">
        <BrowserSurfacePicker mobile />
        <span class:governed-browser-status-agent={agentDriving} class:governed-browser-status-attention={awaitingOperator} class="governed-browser-status-dot"></span>
        <span class="min-w-0 flex-1 truncate text-xs">
          {agentDriving ? "Medousa is browsing" : awaitingOperator ? "Needs you" : "You are browsing"}
        </span>
        {#if agentDriving || awaitingOperator}
          <button type="button" class="btn btn-xs variant-soft-primary" onclick={() => void governedBrowser.takeControl()}>Take over</button>
        {:else}
          <button type="button" class="btn btn-xs variant-soft-surface" onclick={() => void governedBrowser.handBackToAgent()}>Hand back</button>
        {/if}
      </div>
      <div class="mobile-browser-url-row">
        <button type="button" class="mobile-chrome-icon shrink-0" aria-label="Back" disabled={!governedBrowser.canGoBack} onclick={() => void governedBrowser.goBack()}>
          <ArrowLeft size={18} strokeWidth={1.75} />
        </button>
        <form class="mobile-browser-url-dock" onsubmit={navigate}>
          <input
            bind:this={urlInput}
            class="mobile-browser-url-pill min-w-0 flex-1 rounded-full border-0 bg-transparent text-left text-base shadow-none outline-none"
            type="text"
            enterkeyhint="go"
            aria-label="Workshop browser address"
            placeholder="Search or enter URL"
            bind:value={governedBrowser.urlDraft}
            spellcheck="false"
            autocomplete="off"
            autocorrect="off"
            autocapitalize="off"
          />
        </form>
        <button type="button" class="mobile-chrome-icon shrink-0" aria-label="Reload" disabled={governedBrowser.activeUrl === "about:blank"} onclick={() => void governedBrowser.reload()}>
          {#if governedBrowser.frameLoading}
            <LoaderCircle class="animate-spin" size={18} strokeWidth={1.75} />
          {:else}
            <RefreshCw size={18} strokeWidth={1.75} />
          {/if}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .governed-browser-panel {
    position: relative;
    overflow: hidden;
  }

  .governed-browser-tab-strip,
  .governed-browser-handoff,
  .governed-browser-mobile-state {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.5rem;
  }

  .governed-browser-tab-strip {
    min-height: 2rem;
    border-bottom: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.22);
    padding: 0.35rem 0.75rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.75rem;
  }

  .governed-browser-profile {
    flex-shrink: 0;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.68rem;
  }

  .governed-browser-handoff {
    min-height: 2rem;
    border-bottom: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.22);
    padding: 0.3rem 0.75rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.72rem;
  }

  .governed-browser-status-dot {
    height: 0.5rem;
    width: 0.5rem;
    flex-shrink: 0;
    border-radius: 999px;
    background: rgb(52 211 153);
  }

  .governed-browser-status-agent {
    background: rgb(var(--color-primary-400));
  }

  .governed-browser-status-attention {
    background: rgb(251 191 36);
  }

  .governed-browser-error {
    position: absolute;
    right: 0.75rem;
    bottom: 0.75rem;
    z-index: 8;
    display: flex;
    max-width: min(34rem, calc(100% - 1.5rem));
    align-items: center;
    gap: 0.75rem;
    border: 1px solid rgb(244 114 182 / 0.35);
    border-radius: 0.75rem;
    padding: 0.55rem 0.7rem;
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.94);
    color: rgb(244 114 182);
    font-size: 0.72rem;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.28);
  }

  .governed-browser-error span {
    min-width: 0;
    flex: 1 1 auto;
  }

  .governed-browser-error button {
    color: rgb(var(--theme-text-secondary));
  }

  .governed-browser-error-mobile {
    right: 0.65rem;
    bottom: 8.5rem;
    left: 0.65rem;
    max-width: none;
  }

  .governed-browser-mobile-chrome {
    position: fixed;
    left: max(0.75rem, env(safe-area-inset-left, 0px));
    right: max(0.75rem, env(safe-area-inset-right, 0px));
    bottom: calc(env(safe-area-inset-bottom, 0px) + 0.55rem);
    z-index: 45;
    pointer-events: auto;
    border: 1px solid rgb(var(--color-surface-500) / 0.34);
    border-radius: 22px;
    background-color: rgb(var(--color-surface-900) / 0.92);
    box-shadow: 0 10px 36px rgb(0 0 0 / 0.28), inset 0 1px 0 rgb(255 255 255 / 0.06);
    backdrop-filter: blur(28px) saturate(1.35);
    -webkit-backdrop-filter: blur(28px) saturate(1.35);
  }

  .governed-browser-mobile-state {
    padding: 0 0.15rem 0.4rem;
    color: rgb(var(--theme-text-secondary));
  }
</style>
