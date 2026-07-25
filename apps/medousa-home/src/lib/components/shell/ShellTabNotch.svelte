<script lang="ts">
  /**
   * Titlebar tab notch — compact active-pane tabs; opens into a quiet fused
   * pane map the same width as the notch (not a full-shell takeover).
   */
  import DesktopMarks from "$lib/components/layout/DesktopMarks.svelte";
  import ShellTabNotchDrawer from "$lib/components/shell/ShellTabNotchDrawer.svelte";
  import ShellTabStrip from "$lib/components/shell/ShellTabStrip.svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { MAX_SHELL_PANES } from "$lib/types/shellTabs";
  import { ChevronDown, Columns2, Rows2, SquareX } from "@lucide/svelte";
  import { tick } from "svelte";

  let open = $state(false);
  let notchEl = $state<HTMLDivElement | null>(null);
  let drawerEl = $state<HTMLDivElement | null>(null);

  const groupId = $derived(shellTabs.activeGroupId);
  const activeTabs = $derived(shellTabs.tabsForGroup(groupId));
  const paneCount = $derived(shellTabs.paneCount);
  const canSplit = $derived(paneCount < MAX_SHELL_PANES);
  const canMergePane = $derived(paneCount > 1);
  const activeDesktopName = $derived(
    shellTabs.desktops.find((d) => d.id === shellTabs.activeDesktopId)?.name ?? "Main",
  );

  function close() {
    open = false;
  }

  function toggle() {
    open = !open;
  }

  function onTabSettled(info: { tabId: string; didMove: boolean }) {
    if (!info.didMove) close();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  /** Flush-attach under the notch — same width, quiet extension. */
  function placeFusedDrawer() {
    if (!notchEl || !drawerEl) return;
    const tr = notchEl.getBoundingClientRect();
    const width = Math.max(0, tr.width);
    const maxH = Math.min(
      paneCount <= 1 ? 10 * 16 : 18 * 16,
      window.innerHeight * 0.42,
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

  $effect(() => {
    if (!open || !notchEl || !drawerEl) return;
    void paneCount;
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
  class:shell-tab-notch--multi={paneCount > 1}
  data-debug-label="shell-tab-notch"
>
  <div class="shell-tab-notch-body min-w-0">
    {#if open}
      <span class="shell-tab-notch-open-label">
        {activeDesktopName}
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
    {#if open}
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
      <span class="shell-tab-notch-rule" aria-hidden="true"></span>
    {:else}
      <span class="shell-tab-notch-rule" aria-hidden="true"></span>
    {/if}
    <DesktopMarks density="notch" />
    <button
      type="button"
      class="shell-tab-notch-expand"
      title={open ? "Collapse" : "Show panes"}
      aria-label={open ? "Collapse panes" : "Show panes"}
      aria-expanded={open}
      aria-haspopup="dialog"
      onclick={toggle}
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
    <ShellTabNotchDrawer bind:sheetEl={drawerEl} {onTabSettled} />
  </BodyPortal>
{/if}

<style>
  .shell-tab-notch {
    display: flex;
    width: min(38rem, 52vw);
    max-width: 100%;
    height: 32px;
    flex: 0 1 auto;
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
    color: rgb(var(--color-surface-450, var(--color-surface-500)));
  }

  .shell-tab-notch-open-label {
    padding: 0 0.15rem;
    color: rgb(var(--color-surface-200));
    font-size: 0.75rem;
    font-weight: 550;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }

  .shell-tab-notch-open-meta {
    color: rgb(var(--color-surface-500));
    font-weight: 450;
  }

  .shell-tab-notch-empty {
    padding: 0 0.25rem;
    color: rgb(var(--color-surface-500));
    font-size: 0.6875rem;
    white-space: nowrap;
  }

  .shell-tab-notch-trailing {
    display: inline-flex;
    align-items: center;
    gap: 0.15rem;
    padding-right: 0.05rem;
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
    color: rgb(var(--color-surface-400));
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

  .shell-tab-notch--open .shell-tab-notch-expand {
    color: rgb(var(--color-surface-100));
  }

  .shell-tab-notch--open :global(.shell-tab-notch-expand-icon) {
    transform: rotate(180deg);
  }

  .shell-tab-notch-scrim {
    position: fixed;
    inset: 0;
    z-index: 140;
  }
</style>
