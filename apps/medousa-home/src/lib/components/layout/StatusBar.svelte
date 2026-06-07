<script lang="ts">
  import type { DaemonHealth } from "$lib/daemon";

  interface Props {
    health: DaemonHealth | null;
    revision: number;
    inMotionCount: number;
    activeSurface: string;
  }

  let { health, revision, inMotionCount, activeSurface }: Props = $props();
</script>

<footer
  class="flex h-7 shrink-0 items-center justify-between gap-4 border-t border-surface-500/20 bg-surface-900/90 px-3 text-[11px] text-surface-400"
  aria-label="Workshop status"
>
  <span class="truncate {health?.ok ? 'text-success-400' : 'text-warning-400'}">
    {health?.message ?? "checking daemon…"}
  </span>

  <div class="flex shrink-0 items-center gap-3">
    {#if health?.backend}
      <span class="hidden sm:inline">{health.backend}</span>
    {/if}
    <span>rev {revision}</span>
    <span>{inMotionCount} in motion</span>
    <span class="capitalize text-surface-500">{activeSurface}</span>
  </div>
</footer>
