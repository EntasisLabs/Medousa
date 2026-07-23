<script lang="ts">
  import type { DaemonHealth } from "$lib/daemon";
  import type { WorkCard } from "$lib/types/workspace";
  import WorkMotionPeek from "$lib/components/layout/WorkMotionPeek.svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import { formatShortcut } from "$lib/platform";
  import { environment } from "$lib/stores/environment.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { Activity } from "@lucide/svelte";

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

  const showWorkshopSwitcher = $derived(
    Boolean(workshops.activeLabel) && (!minimal || continuity || workshops.hasMultipleWorkshops),
  );
  const showLayoutSwitcher = $derived(
    (environment.spec?.layoutPresets?.length ?? 0) > 1,
  );

  let motionPeekOpen = $state(false);

  const quiet = $derived(minimal || continuity);

  const statusLabel = $derived(
    health?.ok ? "Connected" : health ? "Offline" : "Connecting…",
  );

  const statusDotClass = $derived(
    health?.ok
      ? "workshop-status-dot workshop-status-dot-live"
      : health
        ? "workshop-status-dot workshop-status-dot-warning"
        : "workshop-status-dot workshop-status-dot-muted",
  );

  const statusTextClass = $derived(
    quiet
      ? health?.ok
        ? "text-surface-500"
        : "text-warning-400/80"
      : health?.ok
        ? "text-surface-400"
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

  function toggleMotionPeek() {
    motionPeekOpen = !motionPeekOpen;
  }

  function closeMotionPeek() {
    motionPeekOpen = false;
  }

  async function handleSelectMotion(id: string) {
    closeMotionPeek();
    await onSelectMotion?.(id);
  }

  function onFooterKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && motionPeekOpen) {
      event.preventDefault();
      closeMotionPeek();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<footer
  class="workshop-status relative flex h-8 shrink-0 items-center gap-3 px-3 text-[11px]"
  class:workshop-status--quiet={quiet}
  class:workshop-status--peek-open={motionPeekOpen}
  aria-label="Medousa status"
  data-debug-label="status-bar"
  onkeydown={onFooterKeydown}
>
  <span class="workshop-status-whisper shrink-0 {statusTextClass}">
    <span class={statusDotClass} aria-hidden="true"></span>
    <span class="truncate">{statusLabel}</span>
    {#if showWorkshopSwitcher}
      <span class="text-surface-600">·</span>
      <WorkshopSwitcherCompact variant="status" />
    {/if}
    {#if showLayoutSwitcher}
      <span class="text-surface-600">·</span>
      <EnvironmentPresetSwitcher variant="status" />
    {/if}
  </span>

  <div class="min-w-0 flex-1"></div>

  <div class="flex shrink-0 items-center gap-2.5 text-surface-500">
    {#if !quiet}
      {#if onOpenCron}
        <button
          type="button"
          class="workshop-status-action"
          onclick={onOpenCron}
        >
          {cronActiveCount} automations
        </button>
      {:else if cronTotalCount > 0}
        <span>{cronActiveCount} cron</span>
      {/if}
      {#if deliveryLabel}
        <span
          class={pendingDeliveries && pendingDeliveries > 0
            ? "text-warning-400/85"
            : ""}
        >
          {deliveryLabel}
        </span>
      {/if}
      {#if tickLabel}
        <span class="text-surface-600">{tickLabel}</span>
      {/if}
      {#if onOpenRuntime}
        <button
          type="button"
          class="workshop-status-action"
          onclick={onOpenRuntime}
        >
          Runtime
        </button>
      {/if}
    {/if}

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

    {#if shellTabs.desktops.length > 0}
      <button
        type="button"
        class="workshop-status-action max-w-[9rem] truncate"
        title={
          shellTabs.desktops.length > 1
            ? `Workspace: ${shellTabs.activeDesktopName} (click to cycle)`
            : `Workspace: ${shellTabs.activeDesktopName}`
        }
        onclick={() => shellTabs.cycleDesktop(1)}
      >
        {shellTabs.activeDesktopName}
      </button>
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
