<script lang="ts">
  import { Globe } from "@lucide/svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";

  interface Props {
    groupId: string;
  }

  let { groupId }: Props = $props();
</script>

<button
  type="button"
  class="web-pane-idle flex h-full min-h-0 w-full flex-col items-center justify-center gap-2 px-6 text-center"
  onclick={() => shellTabs.focusGroup(groupId)}
>
  <span
    class="inline-flex size-9 items-center justify-center rounded-lg bg-surface-800/70 text-surface-400"
    aria-hidden="true"
  >
    <Globe size={16} strokeWidth={1.75} />
  </span>
  <p class="text-sm font-medium text-surface-200">Browser</p>
  <p class="workshop-faint max-w-xs text-xs leading-relaxed">
    Hosted in another pane — click to bring it here.
  </p>
</button>
