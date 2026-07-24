<script lang="ts">
  import type { DaemonHealth } from "$lib/daemon";
  import type { WorkCard } from "$lib/types/workspace";
  import WorkMotionPeek from "$lib/components/layout/WorkMotionPeek.svelte";
  import StatusActivityPulse from "$lib/components/layout/StatusActivityPulse.svelte";
  import StatusContextualSlot from "$lib/components/layout/StatusContextualSlot.svelte";
  import StatusDesktopStrip from "$lib/components/layout/StatusDesktopStrip.svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import { formatShortcut } from "$lib/platform";
  import { environment } from "$lib/stores/environment.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    Activity,
    LoaderCircle,
    Radio,
    Unplug,
    Workflow,
  } from "@lucide/svelte";

  interface Props {
    health: DaemonHealth | null;
    inMotionCount: number;
    needsAttentionCount: number;
    cronActiveCount?: number;
    cronTotalCount?: number;
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
  /** Word only when not fine — the radio icon carries “Connected”. */
  const statusLabel = $derived(
    health?.ok ? null : health ? "Offline" : "Connecting…",
  );

  let motionPeekOpen = $state(false);

  const quiet = $derived(minimal || continuity);

  const connectionTitle = $derived(
    connectionOk ? "Connected" : statusLabel ?? "Connecting…",
  );

  const connectionToneClass = $derived(
    connectionOk
      ? "text-surface-500"
      : health
        ? quiet
          ? "text-warning-400/80"
          : "text-warning-400/90"
        : "text-surface-500",
  );

  const showMotion = $derived(inMotionCount > 0 && Boolean(onSelectMotion));

  /** enabled / total — same ratio as Automations panel. */
  const automationsRatio = $derived(`${cronActiveCount}/${cronTotalCount}`);
  const automationsTitle = $derived(
    `${automationsRatio} automations enabled`,
  );

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
  class="workshop-status relative flex h-8 shrink-0 items-center gap-3.5 px-3.5 text-[11px]"
  class:workshop-status--quiet={quiet}
  class:workshop-status--peek-open={motionPeekOpen}
  aria-label="Medousa status"
  data-debug-label="status-bar"
  onkeydown={onFooterKeydown}
>
  <div class="workshop-status-cluster shrink-0">
    {#if onOpenRuntime}
      <button
        type="button"
        class="workshop-status-workshop {connectionToneClass}"
        title={connectionTitle}
        aria-label={connectionTitle}
        onclick={onOpenRuntime}
      >
        {#if connectionOk}
          <Radio size={13} strokeWidth={1.75} class="shrink-0 opacity-80" aria-hidden="true" />
          <span class="truncate">Connected</span>
        {:else if health}
          <Unplug size={13} strokeWidth={1.75} class="shrink-0 opacity-80" aria-hidden="true" />
          <span class="truncate">{statusLabel}</span>
        {:else}
          <LoaderCircle
            size={13}
            strokeWidth={1.75}
            class="shrink-0 animate-spin opacity-80"
            aria-hidden="true"
          />
          <span class="truncate">{statusLabel}</span>
        {/if}
      </button>
    {:else}
      <span
        class="workshop-status-workshop workshop-status-workshop--static {connectionToneClass}"
        title={connectionTitle}
        aria-label={connectionTitle}
      >
        {#if connectionOk}
          <Radio size={13} strokeWidth={1.75} class="shrink-0 opacity-80" aria-hidden="true" />
          <span class="truncate">Connected</span>
        {:else if health}
          <Unplug size={13} strokeWidth={1.75} class="shrink-0 opacity-80" aria-hidden="true" />
          <span class="truncate">{statusLabel}</span>
        {:else}
          <LoaderCircle
            size={13}
            strokeWidth={1.75}
            class="shrink-0 animate-spin opacity-80"
            aria-hidden="true"
          />
          <span class="truncate">{statusLabel}</span>
        {/if}
      </span>
    {/if}
    {#if showWorkshopSwitcher}
      <WorkshopSwitcherCompact variant="status" />
    {/if}
    {#if showLayoutSwitcher}
      <EnvironmentPresetSwitcher variant="status" />
    {/if}
  </div>

  <StatusActivityPulse />

  <!-- Keeps connection/activity left and contextual + desktops + ⌘K pinned right. -->
  <div class="status-bar-mid min-w-0 flex-1" aria-hidden="true"></div>

  <div class="status-bar-trailing flex min-w-0 shrink-0 items-center gap-3 text-surface-500">
    <StatusContextualSlot />

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

    {#if !quiet}
      <StatusDesktopStrip />
    {/if}

    {#if !quiet && onOpenCron}
      <button
        type="button"
        class="status-automations-btn"
        title={automationsTitle}
        aria-label={automationsTitle}
        onclick={onOpenCron}
      >
        <Workflow size={12} strokeWidth={1.85} class="shrink-0 opacity-80" aria-hidden="true" />
        <span class="tabular-nums">{automationsRatio}</span>
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

<style>
  .status-bar-mid {
    min-width: 0.5rem;
  }

  .workshop-status-cluster {
    display: inline-flex;
    min-width: 0;
    align-items: center;
    gap: 0.55rem;
  }

  .status-bar-trailing {
    justify-content: flex-end;
  }

  :global(.workshop-status-workshop--static) {
    pointer-events: none;
  }

  .status-automations-btn {
    display: inline-flex;
    max-width: 9rem;
    min-width: 0;
    align-items: center;
    gap: 0.35rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.15rem 0.35rem;
    color: inherit;
    font: inherit;
    transition:
      color 140ms ease,
      background-color 140ms ease;
  }

  .status-automations-btn:hover {
    background: rgb(var(--color-surface-800) / 0.55);
    color: rgb(var(--color-surface-200));
  }
</style>
