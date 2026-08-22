<script lang="ts">
  import "$lib/styles/shell-tabs.postcss";
  import ShellTabNotchDrawer from "$lib/components/shell/ShellTabNotchDrawer.svelte";
  import ShellTabNotchSearch from "$lib/components/shell/ShellTabNotchSearch.svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import { ChevronDown, PanelsTopLeft, Search } from "@lucide/svelte";
  import { tick } from "svelte";

  type ControlMode = "closed" | "panes" | "search";

  let mode = $state<ControlMode>("closed");
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let drawerEl = $state<HTMLDivElement | null>(null);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let searchPanel = $state<{
    moveHighlight: (delta: number) => Promise<void>;
    confirmHighlight: () => Promise<void>;
  } | null>(null);
  let searchQuery = $state("");

  const open = $derived(mode !== "closed");
  const desktopIndex = $derived(
    Math.max(0, shellTabs.desktops.findIndex((desktop) => desktop.id === shellTabs.activeDesktopId)),
  );

  function close() {
    mode = "closed";
    searchQuery = "";
  }

  function togglePanes() {
    mode = mode === "panes" ? "closed" : "panes";
    searchQuery = "";
  }

  async function openSearch() {
    mode = "search";
    await tick();
    searchInputEl?.focus();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (open && event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  function onSearchKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      void searchPanel?.moveHighlight(event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      void searchPanel?.confirmHighlight();
    }
  }

  function placeDrawer() {
    if (!triggerEl || !drawerEl) return;
    const trigger = triggerEl.getBoundingClientRect();
    const pad = 8;
    const width = Math.min(36 * 16, window.innerWidth - pad * 2);
    const left = Math.max(pad, Math.min(trigger.right - width, window.innerWidth - width - pad));
    const maxHeight = Math.min(28 * 16, window.innerHeight - trigger.bottom - pad * 2);
    drawerEl.style.position = "fixed";
    drawerEl.style.left = `${Math.round(left)}px`;
    drawerEl.style.top = `${Math.round(trigger.bottom + 6)}px`;
    drawerEl.style.width = `${Math.round(width)}px`;
    drawerEl.style.maxWidth = `${Math.round(width)}px`;
    drawerEl.style.maxHeight = `${Math.round(maxHeight)}px`;
    drawerEl.style.zIndex = "145";
  }

  $effect(() => {
    const bar = triggerEl?.closest(".app-titlebar");
    if (!bar) return;
    bar.classList.toggle("app-titlebar--notch-open", open);
    return () => bar.classList.remove("app-titlebar--notch-open");
  });

  $effect(() => {
    if (!open) return;
    void pushBrowserPopoverOverlay();
    return () => void popBrowserPopoverOverlay();
  });

  $effect(() => {
    if (!open || !triggerEl || !drawerEl) return;
    void mode;
    void shellTabs.activeDesktopId;
    void shellTabs.splitRoot;
    let frame = 0;
    const place = () => {
      placeDrawer();
      frame = window.requestAnimationFrame(placeDrawer);
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
    };
  });
</script>

<svelte:window onkeydown={onWindowKeydown} />

<button
  bind:this={triggerEl}
  type="button"
  class="shell-workspace-trigger"
  class:shell-workspace-trigger--open={open}
  title="Customize {shellTabs.activeDesktopName}"
  aria-label="Customize desktop {desktopIndex + 1}, {shellTabs.activeDesktopName}"
  aria-expanded={open}
  aria-haspopup="dialog"
  onclick={togglePanes}
>
  <PanelsTopLeft size={15} strokeWidth={1.75} aria-hidden="true" />
  <span>{desktopIndex + 1}</span>
  <ChevronDown size={13} strokeWidth={2} aria-hidden="true" />
</button>

{#if open}
  <BodyPortal>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="shell-tab-notch-scrim" role="presentation" onclick={close}></div>
    {#if mode === "search"}
      <!-- svelte-ignore a11y_interactive_supports_focus -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        bind:this={drawerEl}
        class="shell-tab-notch-drawer shell-tab-notch-drawer--search"
        role="dialog"
        aria-label="Search open tabs"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
      >
        <label class="shell-workspace-search-field">
          <Search size={14} strokeWidth={1.8} aria-hidden="true" />
          <input
            bind:this={searchInputEl}
            bind:value={searchQuery}
            type="search"
            placeholder="Search tabs across desktops…"
            aria-label="Search tabs across desktops"
            autocomplete="off"
            spellcheck="false"
            onkeydown={onSearchKeydown}
          />
        </label>
        <ShellTabNotchSearch bind:this={searchPanel} bind:query={searchQuery} onPick={close} />
      </div>
    {:else}
      <ShellTabNotchDrawer
        bind:sheetEl={drawerEl}
        onSearch={() => void openSearch()}
        onTabSettled={(info) => {
          if (!info.didMove) close();
        }}
      />
    {/if}
  </BodyPortal>
{/if}

<style>
  .shell-workspace-trigger {
    display: inline-flex;
    height: 28px;
    flex: 0 0 auto;
    align-items: center;
    gap: 0.38rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.2);
    border-radius: 0.42rem;
    background: rgb(var(--color-surface-900) / 0.3);
    padding: 0 0.5rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.72rem;
    font-weight: 550;
    transition: background-color 120ms ease, border-color 120ms ease, color 120ms ease;
  }

  .shell-workspace-trigger:hover,
  .shell-workspace-trigger--open {
    border-color: rgb(var(--color-surface-500) / 0.34);
    background: rgb(var(--color-surface-800) / 0.6);
    color: rgb(var(--color-surface-100));
  }

  .shell-workspace-trigger--open :global(svg:last-child) {
    transform: rotate(180deg);
  }

  .shell-workspace-search-field {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 0.5rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.2);
    padding: 0.7rem 0.8rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .shell-workspace-search-field input {
    min-width: 0;
    flex: 1 1 auto;
    appearance: none;
    border: 0;
    background: transparent;
    color: rgb(var(--color-surface-100));
    font-size: 0.75rem;
    outline: none;
  }

  .shell-workspace-search-field input::placeholder {
    color: rgb(var(--theme-text-quiet));
  }

  .shell-workspace-search-field input::-webkit-search-cancel-button {
    display: none;
  }
</style>
