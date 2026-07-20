<script lang="ts">
  import { FolderTree } from "@lucide/svelte";

  interface Props {
    onExpand: () => void;
    /** Short vertical rail label (e.g. Vault, Browse). */
    label?: string;
  }

  let { onExpand, label = "Vault" }: Props = $props();
</script>

<button
  type="button"
  class="workshop-rail flex h-full w-7 shrink-0 flex-col items-center gap-2 border-r-2 py-3 transition hover:bg-surface-700"
  title="Show workspace browser"
  aria-label="Expand workspace browser"
  onclick={onExpand}
>
  <FolderTree size={16} strokeWidth={1.75} class="text-surface-400" />
  <span
    class="text-[9px] font-medium uppercase tracking-widest text-surface-500"
    style="writing-mode: vertical-rl"
  >
    {label}
  </span>
</button>
