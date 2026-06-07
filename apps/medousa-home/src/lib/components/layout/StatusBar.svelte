<script lang="ts">
  import type { DaemonHealth } from "$lib/daemon";

  interface Props {
    health: DaemonHealth | null;
    inMotionCount: number;
    needsAttentionCount: number;
  }

  let { health, inMotionCount, needsAttentionCount }: Props = $props();

  const statusLabel = $derived(
    health?.ok ? "Connected" : health ? "Offline" : "Connecting…",
  );
</script>

<footer
  class="flex h-7 shrink-0 items-center justify-between gap-4 border-t border-surface-500/20 bg-surface-900/90 px-3 text-[11px] text-surface-400"
  aria-label="Workshop status"
>
  <span class="truncate {health?.ok ? 'text-success-400' : 'text-warning-400'}">
    {statusLabel}
  </span>

  <div class="flex shrink-0 items-center gap-3">
    <span>{inMotionCount} in motion</span>
    {#if needsAttentionCount > 0}
      <span class="text-warning-400">
        {needsAttentionCount} need attention
      </span>
    {/if}
  </div>
</footer>
