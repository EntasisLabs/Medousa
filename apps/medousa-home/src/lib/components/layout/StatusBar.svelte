<script lang="ts">
  import type { DaemonHealth } from "$lib/daemon";

  interface Props {
    health: DaemonHealth | null;
    inMotionCount: number;
    needsAttentionCount: number;
    cronActiveCount?: number;
    cronTotalCount?: number;
    pendingDeliveries?: number | null;
    lastTickAt?: string | null;
    /** Whisper connection only — for Chat tab focus. */
    minimal?: boolean;
    onOpenRuntime?: () => void;
    onOpenCron?: () => void;
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
    onOpenRuntime,
    onOpenCron,
  }: Props = $props();

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
    minimal
      ? health?.ok
        ? "text-surface-600"
        : "text-warning-400/90"
      : health?.ok
        ? "text-primary-300"
        : "text-warning-400",
  );

  const deliveryLabel = $derived.by(() => {
    if (pendingDeliveries === null) return null;
    if (pendingDeliveries > 0) return `${pendingDeliveries} pending delivery`;
    return "delivery ok";
  });

  const tickLabel = $derived.by(() => {
    if (!lastTickAt) return null;
    const date = new Date(lastTickAt);
    if (Number.isNaN(date.getTime())) return null;
    return `tick ${date.toLocaleTimeString()}`;
  });
</script>

<footer
  class="workshop-status flex shrink-0 items-center justify-between gap-4 px-3 text-[11px] {minimal
    ? 'h-5 border-t border-surface-700/20 text-surface-600'
    : 'h-8'}"
  aria-label="Medousa status"
>
  <span class="workshop-status-whisper {statusTextClass}">
    <span class={statusDotClass} aria-hidden="true"></span>
    <span class="truncate">{statusLabel}</span>
  </span>

  {#if !minimal}
    <div class="flex shrink-0 items-center gap-3">
    {#if onOpenCron}
      <button
        type="button"
        class="text-surface-300 transition hover:text-primary-300"
        onclick={onOpenCron}
      >
        {cronActiveCount} cron active
      </button>
    {:else if cronTotalCount > 0}
      <span>{cronActiveCount} cron active</span>
    {/if}
    {#if deliveryLabel}
      <span class={pendingDeliveries && pendingDeliveries > 0 ? "text-warning-400" : ""}>
        {deliveryLabel}
      </span>
    {/if}
    {#if tickLabel}
      <span>{tickLabel}</span>
    {/if}
    <span>{inMotionCount} in motion</span>
    {#if needsAttentionCount > 0}
      <span class="text-warning-400">
        {needsAttentionCount} need attention
      </span>
    {/if}
    {#if onOpenRuntime}
      <button
        type="button"
        class="text-primary-300 transition hover:text-primary-200"
        onclick={onOpenRuntime}
      >
        Runtime
      </button>
    {/if}
  </div>
  {/if}
</footer>
