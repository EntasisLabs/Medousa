<script lang="ts">
  import type { DaemonHealth } from "$lib/daemon";
  import type { WorkCard } from "$lib/types/workspace";
  import WorkMotionPeek from "$lib/components/layout/WorkMotionPeek.svelte";
  import StatusActivityPulse from "$lib/components/layout/StatusActivityPulse.svelte";
  import StatusContextualSlot from "$lib/components/layout/StatusContextualSlot.svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import { formatShortcut } from "$lib/platform";
  import { environment } from "$lib/stores/environment.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { placeToolbarPopover } from "$lib/utils/railPopover";
  import { Activity, MoreHorizontal } from "@lucide/svelte";
  import { tick } from "svelte";

  interface Props {
    health: DaemonHealth | null;
    inMotionCount: number;
    needsAttentionCount: number;
    cronActiveCount?: number;
    cronTotalCount?: number;
    pendingDeliveries?: number | null;
    lastTickAt?: string | null;
    /** Whisper connection styling — for Chat tab focus. */
    minimal?: boolean;
    /** Library continuity — connection + brain name styling. */
    continuity?: boolean;
    motionCards?: WorkCard[];
    selectedMotionId?: string | null;
    onSelectMotion?: (id: string) => void | Promise<void>;
    onOpenRuntime?: () => void;
    onOpenCron?: () => void;
    onOpenSpotlight?: () => void;
  }

  let {
    health,
    inMotionCount,
    needsAttentionCount,
    cronActiveCount = 0,
    cronTotalCount = 0,
    pendingDeliveries = null,
    lastTickAt = null,
    minimal = false,
    continuity = false,
    motionCards = [],
    selectedMotionId = null,
    onSelectMotion,
    onOpenRuntime,
    onOpenCron,
    onOpenSpotlight,
  }: Props = $props();

  /** Only when there’s somewhere to switch — otherwise it’s jewelry. */
  const showWorkshopSwitcher = $derived(workshops.hasMultipleWorkshops);
  const showLayoutSwitcher = $derived(
    (environment.spec?.layoutPresets?.length ?? 0) > 1,
  );
  const connectionOk = $derived(Boolean(health?.ok));
  /** Word only when not fine — the dot carries “Connected”. */
  const statusLabel = $derived(
    health?.ok ? null : health ? "Offline" : "Connecting…",
  );

  let motionPeekOpen = $state(false);
  let overflowOpen = $state(false);
  let overflowTriggerEl = $state<HTMLButtonElement | null>(null);
  let overflowMenuEl = $state<HTMLDivElement | null>(null);

  const quiet = $derived(minimal || continuity);

  const statusDotClass = $derived(
    connectionOk
      ? "workshop-status-dot workshop-status-dot-live"
      : health
        ? "workshop-status-dot workshop-status-dot-warning"
        : "workshop-status-dot workshop-status-dot-muted",
  );

  const statusTextClass = $derived(
    connectionOk
      ? "text-surface-500"
      : quiet
        ? "text-warning-400/80"
        : "text-warning-400/90",
  );

  const deliveryLabel = $derived.by(() => {
    if (pendingDeliveries === null) return null;
    if (pendingDeliveries > 0) return `${pendingDeliveries} pending`;
    return "delivery ok";
  });

  const tickLabel = $derived.by(() => {
    if (!lastTickAt) return null;
    const date = new Date(lastTickAt);
    if (Number.isNaN(date.getTime())) return null;
    return `tick ${date.toLocaleTimeString()}`;
  });

  const showMotion = $derived(inMotionCount > 0 && Boolean(onSelectMotion));

  const hasOverflowItems = $derived(
    Boolean(onOpenCron) ||
      cronTotalCount > 0 ||
      deliveryLabel !== null ||
      tickLabel !== null ||
      Boolean(onOpenRuntime) ||
      shellTabs.desktops.length > 0,
  );

  $effect(() => {
    if (!overflowOpen || !overflowTriggerEl || !overflowMenuEl) return;
    let frame = 0;
    const place = () => {
      if (!overflowTriggerEl || !overflowMenuEl) return;
      placeToolbarPopover(overflowTriggerEl, overflowMenuEl, {
        prefer: "above",
        width: 220,
        gap: 8,
        pad: 10,
      });
      frame = window.requestAnimationFrame(() => {
        if (!overflowTriggerEl || !overflowMenuEl) return;
        placeToolbarPopover(overflowTriggerEl, overflowMenuEl, {
          prefer: "above",
          width: 220,
          gap: 8,
          pad: 10,
        });
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
    };
  });

  function toggleMotionPeek() {
    overflowOpen = false;
    motionPeekOpen = !motionPeekOpen;
  }

  function closeMotionPeek() {
    motionPeekOpen = false;
  }

  function toggleOverflow() {
    motionPeekOpen = false;
    overflowOpen = !overflowOpen;
  }

  function closeOverflow() {
    overflowOpen = false;
  }

  async function handleSelectMotion(id: string) {
    closeMotionPeek();
    await onSelectMotion?.(id);
  }

  function onFooterKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      if (motionPeekOpen) {
        event.preventDefault();
        closeMotionPeek();
      } else if (overflowOpen) {
        event.preventDefault();
        closeOverflow();
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<footer
  class="workshop-status relative flex h-8 shrink-0 items-center gap-2.5 px-3 text-[11px]"
  class:workshop-status--quiet={quiet}
  class:workshop-status--peek-open={motionPeekOpen || overflowOpen}
  aria-label="Medousa status"
  data-debug-label="status-bar"
  onkeydown={onFooterKeydown}
>
  <span
    class="workshop-status-whisper shrink-0 {statusTextClass}"
    title={connectionOk ? "Connected" : statusLabel ?? "Connecting…"}
    aria-label={connectionOk ? "Connected" : statusLabel ?? "Connecting…"}
  >
    <span class={statusDotClass} aria-hidden="true"></span>
    {#if statusLabel}
      <span class="truncate">{statusLabel}</span>
    {/if}
    {#if showWorkshopSwitcher}
      <span class="text-surface-600" aria-hidden="true">·</span>
      <WorkshopSwitcherCompact variant="status" />
    {/if}
    {#if showLayoutSwitcher}
      <span class="text-surface-600" aria-hidden="true">·</span>
      <EnvironmentPresetSwitcher variant="status" />
    {/if}
  </span>

  <StatusActivityPulse />

  <StatusContextualSlot />

  <div class="flex shrink-0 items-center gap-2.5 text-surface-500">
    {#if showMotion}
      <div class="work-motion-control">
        <button
          type="button"
          class="work-motion-trigger"
          class:work-motion-trigger--active={inMotionCount > 0}
          class:work-motion-trigger--open={motionPeekOpen}
          aria-expanded={motionPeekOpen}
          aria-haspopup="dialog"
          title="In-motion work"
          onclick={toggleMotionPeek}
        >
          <Activity
            size={12}
            strokeWidth={2}
            class="work-motion-icon {inMotionCount > 0
              ? 'work-motion-icon--live'
              : ''}"
          />
          <span>{inMotionCount} in motion</span>
          {#if needsAttentionCount > 0}
            <span class="work-motion-attention" title="Needs attention">
              {needsAttentionCount}
            </span>
          {/if}
        </button>

        {#if motionPeekOpen}
          <div
            class="work-motion-peek"
            role="dialog"
            aria-label="In-motion work"
          >
            <WorkMotionPeek
              cards={motionCards}
              selectedId={selectedMotionId}
              onSelect={handleSelectMotion}
            />
          </div>
        {/if}
      </div>
    {:else if !quiet && needsAttentionCount > 0}
      <span class="text-warning-400/85">{needsAttentionCount} need attention</span>
    {/if}

    {#if !quiet && hasOverflowItems}
      <div class="status-overflow">
        <button
          bind:this={overflowTriggerEl}
          type="button"
          class="status-overflow-trigger"
          class:status-overflow-trigger--open={overflowOpen}
          title="More status"
          aria-label="More status"
          aria-expanded={overflowOpen}
          aria-haspopup="menu"
          onclick={toggleOverflow}
        >
          <MoreHorizontal size={14} strokeWidth={2} />
        </button>

        {#if overflowOpen}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="status-overflow-scrim"
            role="presentation"
            onclick={closeOverflow}
          ></div>
          <div
            bind:this={overflowMenuEl}
            class="status-overflow-menu workshop-rail-sheet"
            role="menu"
            aria-label="More status"
          >
            {#if onOpenCron}
              <button
                type="button"
                class="status-overflow-item"
                role="menuitem"
                onclick={() => {
                  closeOverflow();
                  onOpenCron();
                }}
              >
                {cronActiveCount} automations
              </button>
            {:else if cronTotalCount > 0}
              <span class="status-overflow-item status-overflow-item--static">
                {cronActiveCount} cron
              </span>
            {/if}
            {#if deliveryLabel}
              <span
                class="status-overflow-item status-overflow-item--static"
                class:text-warning-400={pendingDeliveries && pendingDeliveries > 0}
              >
                {deliveryLabel}
              </span>
            {/if}
            {#if tickLabel}
              <span class="status-overflow-item status-overflow-item--static text-surface-500">
                {tickLabel}
              </span>
            {/if}
            {#if onOpenRuntime}
              <button
                type="button"
                class="status-overflow-item"
                role="menuitem"
                onclick={() => {
                  closeOverflow();
                  onOpenRuntime();
                }}
              >
                Runtime
              </button>
            {/if}
            {#if shellTabs.desktops.length > 0}
              <button
                type="button"
                class="status-overflow-item"
                role="menuitem"
                title={
                  shellTabs.desktops.length > 1
                    ? `Workspace: ${shellTabs.activeDesktopName} (click to cycle)`
                    : `Workspace: ${shellTabs.activeDesktopName}`
                }
                onclick={() => {
                  shellTabs.cycleDesktop(1);
                  closeOverflow();
                }}
              >
                {shellTabs.activeDesktopName}
              </button>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    {#if onOpenSpotlight}
      <button
        type="button"
        class="workshop-status-spotlight"
        title="Command spotlight"
        onclick={onOpenSpotlight}
      >
        {formatShortcut("K")}
      </button>
    {/if}
  </div>
</footer>

{#if motionPeekOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="work-motion-scrim"
    role="presentation"
    onclick={closeMotionPeek}
  ></div>
{/if}

<style>
  .status-overflow {
    position: relative;
    display: inline-flex;
  }

  .status-overflow-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: transparent;
    padding: 0.1rem;
    color: rgb(var(--color-surface-500));
    transition: color 140ms ease;
  }

  .status-overflow-trigger:hover,
  .status-overflow-trigger--open {
    color: rgb(var(--color-surface-200));
  }

  .status-overflow-scrim {
    position: fixed;
    inset: 0;
    z-index: 70;
  }

  .status-overflow-menu {
    z-index: 71;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.4rem;
    min-width: 10rem;
    pointer-events: auto;
  }

  .status-overflow-item {
    display: block;
    width: 100%;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    padding: 0.4rem 0.55rem;
    color: rgb(var(--color-surface-200));
    font: inherit;
    text-align: left;
    transition: background 120ms ease;
  }

  .status-overflow-item:hover {
    background: rgb(var(--color-surface-800) / 0.55);
  }

  .status-overflow-item--static {
    color: rgb(var(--color-surface-400));
    cursor: default;
  }

  .status-overflow-item--static:hover {
    background: transparent;
  }
</style>
