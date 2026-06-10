<script lang="ts">
  import { FolderTree } from "@lucide/svelte";

  interface Props {
    onExpand: () => void;
  }

  let { onExpand }: Props = $props();
</script>

<button
  type="button"
  class="workshop-rail flex h-full w-7 shrink-0 flex-col items-center gap-2 border-r-2 py-3 transition hover:bg-surface-700"
  title="Show library browser"
  aria-label="Expand library browser"
  onclick={onExpand}
>
  <FolderTree size={16} strokeWidth={1.75} class="text-surface-400" />
  <span
    class="text-[9px] font-medium uppercase tracking-widest text-surface-500"
    style="writing-mode: vertical-rl"
  >
    Vault
  </span>
</button>
