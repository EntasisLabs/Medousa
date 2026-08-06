<script lang="ts">
  import { Activity } from "@lucide/svelte";

  interface Props {
    onExpand: () => void;
  }

  let { onExpand }: Props = $props();
</script>

<button
  type="button"
  class="workshop-rail flex h-full w-7 shrink-0 flex-col items-center gap-2 py-3 transition hover:bg-surface-700"
  title="Expand activity"
  aria-label="Expand activity panel"
  data-debug-label="activity-collapsed-strip"
  onclick={onExpand}
>
  <Activity size={16} strokeWidth={1.75} class="text-content-tertiary" />
  <span
    class="text-[9px] font-medium uppercase tracking-widest text-content-quiet"
    style="writing-mode: vertical-rl"
  >
    Now
  </span>
</button>
