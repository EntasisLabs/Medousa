<script lang="ts">
  import { Activity } from "@lucide/svelte";

  interface Props {
    onExpand: () => void;
  }

  let { onExpand }: Props = $props();
</script>

<button
  type="button"
  class="workshop-rail flex h-full w-8 shrink-0 flex-col items-center gap-3 py-4 transition hover:bg-surface-700"
  title="Expand activity"
  aria-label="Expand activity panel"
  onclick={onExpand}
>
  <Activity size={16} strokeWidth={1.75} class="text-surface-400" />
  <span
    class="text-[10px] font-medium uppercase tracking-widest text-surface-500"
    style="writing-mode: vertical-rl"
  >
    Activity
  </span>
</button>
