<script lang="ts">
  import type { DaemonHealth } from "$lib/daemon";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    health: DaemonHealth | null;
    onTap?: () => void;
  }

  let { health, onTap }: Props = $props();

  const label = $derived(
    health?.ok ? "Connected" : health ? "Offline" : "Connecting…",
  );
  const tone = $derived(health?.ok ? "text-content-success" : "text-content-warning");
  const motion = $derived(workspace.inMotionCount());
  const attention = $derived(workspace.needsAttentionCount());
</script>

<button
  type="button"
  class="mobile-connection-pill flex min-w-0 flex-1 items-center gap-2 text-left"
  onclick={() => onTap?.()}
>
  <span class="h-2 w-2 shrink-0 rounded-full {health?.ok ? 'bg-success-400' : 'bg-warning-400'}"></span>
  <span class="truncate text-xs font-medium {tone}">{label}</span>
  {#if motion > 0}
    <span class="shrink-0 text-[10px] text-content-tertiary">{motion} in motion</span>
  {/if}
  {#if attention > 0}
    <span class="shrink-0 text-[10px] text-content-warning">{attention} need you</span>
  {/if}
</button>
