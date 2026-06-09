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
  onclick={onExpand}
>
  <Activity size={16} strokeWidth={1.75} class="text-surface-400" />
  <span
    class="text-[9px] font-medium uppercase tracking-widest text-surface-500"
    style="writing-mode: vertical-rl"
  >
    Now
  </span>
</button>
