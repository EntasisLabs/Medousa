<script lang="ts">
  import type { DaemonHealth } from "$lib/daemon";

  interface Props {
    health: DaemonHealth | null;
    inMotionCount: number;
    needsAttentionCount: number;
    pendingDeliveries?: number | null;
    lastTickAt?: string | null;
    onOpenRuntime?: () => void;
  }

  let {
    health,
    inMotionCount,
    needsAttentionCount,
    pendingDeliveries = null,
    lastTickAt = null,
    onOpenRuntime,
  }: Props = $props();

  const statusLabel = $derived(
    health?.ok ? "Connected" : health ? "Offline" : "Connecting…",
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
  class="workshop-status flex h-8 shrink-0 items-center justify-between gap-4 px-3 text-[11px]"
  aria-label="Workshop status"
>
  <span class="truncate {health?.ok ? 'text-success-400' : 'text-warning-400'}">
    {statusLabel}
  </span>

  <div class="flex shrink-0 items-center gap-3">
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
</footer>
