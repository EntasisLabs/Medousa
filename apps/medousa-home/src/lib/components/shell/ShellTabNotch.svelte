<script lang="ts">
  /**
   * Titlebar tab notch — compact active-pane tabs; opens into a quiet fused
   * pane map or cross-desktop tab search (same width as the notch).
   */
  import DesktopMarks from "$lib/components/layout/DesktopMarks.svelte";
  import ShellTabNotchDrawer from "$lib/components/shell/ShellTabNotchDrawer.svelte";
  import ShellTabNotchSearch from "$lib/components/shell/ShellTabNotchSearch.svelte";
  import ShellTabStrip from "$lib/components/shell/ShellTabStrip.svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { MAX_SHELL_PANES } from "$lib/types/shellTabs";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import { ChevronDown, Columns2, Rows2, Search, SquareX } from "@lucide/svelte";
  import { tick } from "svelte";

  type NotchMode = "closed" | "panes" | "search";

  let mode = $state<NotchMode>("closed");
  let notchEl = $state<HTMLDivElement | null>(null);
  let drawerEl = $state<HTMLDivElement | null>(null);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let searchPanel = $state<{
    moveHighlight: (delta: number) => Promise<void>;
    confirmHighlight: () => Promise<void>;
  } | null>(null);
  let searchQuery = $state("");
  let renamingDesktop = $state(false);
  let renameDraft = $state("");
  let renameInputEl = $state<HTMLInputElement | null>(null);

  const groupId = $derived(shellTabs.activeGroupId);
  const activeTabs = $derived(shellTabs.tabsForGroup(groupId));
  const paneCount = $derived(shellTabs.paneCount);
  const canSplit = $derived(paneCount < MAX_SHELL_PANES);
  const canMergePane = $derived(paneCount > 1);
  const open = $derived(mode !== "closed");
  const activeDesktopName = $derived(
    shellTabs.desktops.find((d) => d.id === shellTabs.activeDesktopId)?.name ?? "Main",
  );

  function cancelDesktopRename() {
    renamingDesktop = false;
    renameDraft = "";
  }

  function close() {
    mode = "closed";
    searchQuery = "";
    cancelDesktopRename();
  }

  function togglePanes() {
    const next = mode === "panes" ? "closed" : "panes";
    mode = next;
    searchQuery = "";
    cancelDesktopRename();
  }

  async function beginDesktopRename() {
    renamingDesktop = true;
    renameDraft = activeDesktopName;
    await tick();
    renameInputEl?.focus();
    renameInputEl?.select();
  }

  function commitDesktopRename() {
    if (!renamingDesktop) return;
    const next = renameDraft.trim();
    cancelDesktopRename();
    if (!next || next === activeDesktopName) return;
    shellTabs.renameDesktop(shellTabs.activeDesktopId, next);
  }

  function onRenameKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      commitDesktopRename();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelDesktopRename();
    }
  }

  async function openSearch() {
    mode = "search";
    await tick();
    searchInputEl?.focus();
    searchInputEl?.select();
  }

  function toggleSearch() {
    if (mode === "search") {
      close();
      return;
    }
    void openSearch();
  }

  function onTabSettled(info: { tabId: string; didMove: boolean }) {
    if (!info.didMove) close();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === "Escape") {
      if (renamingDesktop) {
        event.preventDefault();
        cancelDesktopRename();
        return;
      }
      event.preventDefault();
      close();
    }
  }

  function onSearchKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      void searchPanel?.moveHighlight(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      void searchPanel?.moveHighlight(-1);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      void searchPanel?.confirmHighlight();
    }
  }

  /** Flush-attach under the notch — same width, quiet extension. */
  function placeFusedDrawer() {
    if (!notchEl || !drawerEl) return;
    const tr = notchEl.getBoundingClientRect();
    const width = Math.max(0, tr.width);
    const maxH = Math.min(
      mode === "search"
        ? 22 * 16
        : paneCount <= 1
          ? 10 * 16
          : 18 * 16,
      window.innerHeight * (mode === "search" ? 0.5 : 0.42),
    );

    drawerEl.style.position = "fixed";
    drawerEl.style.left = `${Math.round(tr.left)}px`;
    drawerEl.style.top = `${Math.round(tr.bottom)}px`;
    drawerEl.style.width = `${Math.round(width)}px`;
    drawerEl.style.maxWidth = `${Math.round(width)}px`;
    drawerEl.style.height = "auto";
    drawerEl.style.maxHeight = `${Math.round(maxH)}px`;
    drawerEl.style.bottom = "auto";
    drawerEl.style.overflow = "hidden";
    drawerEl.style.zIndex = "145";
  }

  $effect(() => {
    const bar = notchEl?.closest(".app-titlebar");
    if (!bar) return;
    bar.classList.toggle("app-titlebar--notch-open", open);
    return () => bar.classList.remove("app-titlebar--notch-open");
  });

  // Native browser embed paints above DOM — hide it while the fused drawer is open
  // (same pattern as CommandSpotlight / NavRailViewPopover).
  $effect(() => {
    if (!open) return;
    void pushBrowserPopoverOverlay();
    return () => {
      void popBrowserPopoverOverlay();
    };
  });

  $effect(() => {
    if (!open || !notchEl || !drawerEl) return;
    void paneCount;
    void mode;
    void shellTabs.activeDesktopId;
    void shellTabs.splitRoot;
    let frame = 0;
    const place = () => {
      placeFusedDrawer();
      frame = window.requestAnimationFrame(() => placeFusedDrawer());
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

<div
  bind:this={notchEl}
  class="shell-tab-notch"
  class:shell-tab-notch--open={open}
  class:shell-tab-notch--search={mode === "search"}
  class:shell-tab-notch--multi={paneCount > 1}
  data-debug-label="shell-tab-notch"
>
  <div class="shell-tab-notch-body min-w-0">
    {#if mode === "search"}
      <label class="shell-tab-notch-search-field">
        <Search size={13} strokeWidth={1.85} aria-hidden="true" />
        <input
          bind:this={searchInputEl}
          bind:value={searchQuery}
          class="shell-tab-notch-search-input"
          type="search"
          placeholder="Search tabs across desktops…"
          aria-label="Search tabs across desktops"
          autocomplete="off"
          spellcheck="false"
          onkeydown={onSearchKeydown}
        />
      </label>
    {:else if mode === "panes"}
      <span class="shell-tab-notch-open-label">
        {#if renamingDesktop}
          <input
            bind:this={renameInputEl}
            bind:value={renameDraft}
            class="shell-tab-notch-rename-input"
            type="text"
            maxlength={32}
            aria-label="Rename desktop"
            spellcheck="false"
            onkeydown={onRenameKeydown}
            onblur={commitDesktopRename}
          />
        {:else}
          <button
            type="button"
            class="shell-tab-notch-desktop-name"
            title="Double-click to rename"
            aria-label="Desktop {activeDesktopName}. Double-click to rename."
            ondblclick={(event) => {
              event.preventDefault();
              void beginDesktopRename();
            }}
          >
            {activeDesktopName}
          </button>
        {/if}
        <span class="shell-tab-notch-open-meta">
          · {paneCount} pane{paneCount === 1 ? "" : "s"}
        </span>
      </span>
    {:else if activeTabs.length > 0}
      <ShellTabStrip {groupId} variant="titlebar" />
    {:else}
      <span class="shell-tab-notch-empty">No tabs</span>
    {/if}
  </div>

  <div class="shell-tab-notch-trailing shrink-0">
    <div class="shell-tab-notch-pane-actions">
      <button
        type="button"
        class="shell-tab-notch-expand"
        title="Split right"
        aria-label="Split pane right"
        disabled={!canSplit}
        onclick={() => shellTabs.splitActive("right")}
      >
        <Columns2 size={13} strokeWidth={1.85} />
      </button>
      <button
        type="button"
        class="shell-tab-notch-expand"
        title="Split down"
        aria-label="Split pane down"
        disabled={!canSplit}
        onclick={() => shellTabs.splitActive("down")}
      >
        <Rows2 size={13} strokeWidth={1.85} />
      </button>
      <button
        type="button"
        class="shell-tab-notch-expand"
        title="Close pane · merge tabs"
        aria-label="Close pane and merge tabs"
        disabled={!canMergePane}
        onclick={() => shellTabs.closeActiveGroup()}
      >
        <SquareX size={13} strokeWidth={1.85} />
      </button>
    </div>
    <span class="shell-tab-notch-rule" aria-hidden="true"></span>
    <button
      type="button"
      class="shell-tab-notch-expand"
      class:shell-tab-notch-expand--on={mode === "search"}
      title="Search tabs"
      aria-label="Search tabs across desktops"
      aria-pressed={mode === "search"}
      onclick={toggleSearch}
    >
      <Search size={13} strokeWidth={1.85} />
    </button>
    <DesktopMarks density="notch" />
    <button
      type="button"
      class="shell-tab-notch-expand"
      title={mode === "panes" ? "Collapse" : "Show panes"}
      aria-label={mode === "panes" ? "Collapse panes" : "Show panes"}
      aria-expanded={mode === "panes"}
      aria-haspopup="dialog"
      onclick={togglePanes}
    >
      <ChevronDown
        size={14}
        strokeWidth={2}
        class="shell-tab-notch-expand-icon"
        aria-hidden="true"
      />
    </button>
  </div>
</div>

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
        aria-label="Tab search"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
      >
        <ShellTabNotchSearch bind:this={searchPanel} bind:query={searchQuery} onPick={close} />
      </div>
    {:else}
      <ShellTabNotchDrawer bind:sheetEl={drawerEl} {onTabSettled} />
    {/if}
  </BodyPortal>
{/if}

<style>
  .shell-tab-notch {
    display: flex;
    box-sizing: border-box;
    width: min(38rem, 52vw);
    max-width: 100%;
    min-width: 0;
    height: 32px;
    flex: 0 0 auto;
    align-items: center;
    gap: 0.45rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.16);
    border-radius: 0.4rem;
    background: rgb(var(--color-surface-900) / 0.35);
    padding: 0 0.35rem 0 0.5rem;
    transition:
      border-color 160ms ease,
      background-color 160ms ease,
      border-radius 160ms ease;
  }

  .shell-tab-notch--multi:hover {
    border-color: rgb(var(--color-surface-500) / 0.28);
    background: rgb(var(--color-surface-900) / 0.5);
  }

  .shell-tab-notch--open {
    z-index: 146;
    border-color: rgb(var(--color-surface-500) / 0.28);
    border-bottom-color: transparent;
    border-radius: 0.4rem 0.4rem 0 0;
    background: rgb(var(--color-surface-900) / 0.88);
  }

  .shell-tab-notch-body {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    align-items: center;
    overflow: hidden;
    padding-right: 0.2rem;
  }

  .shell-tab-notch-body :global(.shell-tab-strip--titlebar) {
    max-width: 100%;
  }

  .shell-tab-notch-body :global(.shell-tab-chip) {
    max-width: 10rem;
  }

  .shell-tab-notch-body :global(.shell-tab-chip--active) {
    background: rgb(var(--color-surface-700) / 0.9);
    color: rgb(var(--color-surface-50));
  }

  .shell-tab-notch-body :global(.shell-tab-chip--idle) {
    color: rgb(var(--theme-text-tertiary));
  }

  .shell-tab-notch-open-label {
    display: inline-flex;
    min-width: 0;
    align-items: center;
    gap: 0.2rem;
    padding: 0 0.15rem;
    color: rgb(var(--color-surface-200));
    font-size: 0.75rem;
    font-weight: 550;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }

  .shell-tab-notch-desktop-name {
    min-width: 0;
    max-width: 12rem;
    overflow: hidden;
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    font: inherit;
    letter-spacing: inherit;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: default;
  }

  .shell-tab-notch-desktop-name:hover {
    color: rgb(var(--color-surface-50));
  }

  .shell-tab-notch-rename-input {
    min-width: 4.5rem;
    max-width: 12rem;
    appearance: none;
    border: 1px solid rgb(var(--color-surface-500) / 0.45);
    border-radius: 0.25rem;
    background: rgb(var(--color-surface-800) / 0.85);
    padding: 0.05rem 0.3rem;
    color: rgb(var(--color-surface-50));
    font: inherit;
    letter-spacing: inherit;
    outline: none;
    box-shadow: none;
    caret-color: rgb(var(--color-surface-100));
  }

  .shell-tab-notch-rename-input:focus,
  .shell-tab-notch-rename-input:focus-visible {
    border-color: rgb(var(--color-surface-400) / 0.55);
    outline: none;
    box-shadow: none;
  }

  .shell-tab-notch-rename-input::selection {
    background: rgb(var(--color-primary-500) / 0.35);
    color: rgb(var(--color-surface-50));
  }

  .shell-tab-notch-open-meta {
    color: rgb(var(--theme-text-quiet));
    font-weight: 450;
  }

  .shell-tab-notch-empty {
    padding: 0 0.25rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.6875rem;
    white-space: nowrap;
  }

  .shell-tab-notch-search-field {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    align-items: center;
    gap: 0.35rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .shell-tab-notch-search-input {
    min-width: 0;
    flex: 1 1 auto;
    appearance: none;
    border: 0;
    background: transparent;
    color: rgb(var(--color-surface-100));
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: -0.01em;
    outline: none;
    box-shadow: none;
    caret-color: rgb(var(--color-surface-100));
  }

  .shell-tab-notch-search-input:focus,
  .shell-tab-notch-search-input:focus-visible {
    outline: none;
    box-shadow: none;
  }

  .shell-tab-notch-search-input::placeholder {
    color: rgb(var(--theme-text-quiet));
    font-weight: 450;
  }

  .shell-tab-notch-search-input::selection {
    background: rgb(var(--color-primary-500) / 0.35);
    color: rgb(var(--color-surface-50));
  }

  .shell-tab-notch-search-input::-webkit-search-cancel-button {
    display: none;
  }

  .shell-tab-notch-trailing {
    display: inline-flex;
    align-items: center;
    gap: 0.15rem;
    padding-right: 0.05rem;
  }

  .shell-tab-notch-pane-actions {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.15rem;
  }

  .shell-tab-notch-rule {
    width: 1px;
    height: 14px;
    margin-right: 0.1rem;
    background: rgb(var(--color-surface-500) / 0.28);
  }

  .shell-tab-notch-expand {
    display: inline-flex;
    width: 22px;
    height: 22px;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    color: rgb(var(--theme-text-tertiary));
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .shell-tab-notch-expand:hover:not(:disabled) {
    background: rgb(var(--color-surface-800) / 0.55);
    color: rgb(var(--color-surface-100));
  }

  .shell-tab-notch-expand:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .shell-tab-notch-expand--on {
    background: rgb(var(--color-surface-800) / 0.65);
    color: rgb(var(--color-surface-50));
  }

  .shell-tab-notch--open .shell-tab-notch-expand {
    color: rgb(var(--color-surface-100));
  }

  .shell-tab-notch--open :global(.shell-tab-notch-expand-icon) {
    transform: rotate(180deg);
  }

  .shell-tab-notch--search :global(.shell-tab-notch-expand-icon) {
    transform: none;
  }

  .shell-tab-notch-scrim {
    position: fixed;
    inset: 0;
    z-index: 140;
  }
</style>
