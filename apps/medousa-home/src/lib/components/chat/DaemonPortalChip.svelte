<script lang="ts">
  import { connection } from "$lib/stores/connection.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";

  interface Props {
    compact?: boolean;
    class?: string;
  }

  let { compact = false, class: className = "" }: Props = $props();

  const detail = $derived.by(() => {
    if (!connection.health?.ok) return "Connect to your Mac to chat";
    if (runtime.defaultsLoaded) return runtime.modelLabel();
    return "Mac daemon defaults";
  });
</script>

<p
  class="daemon-portal-chip {className}"
  title="This device sends turns to your Mac daemon — model and routing live there."
>
  <span class="daemon-portal-chip-badge">Mac</span>
  {#if compact}
    <span class="truncate">{detail}</span>
  {:else}
    <span class="min-w-0 truncate">Using {detail}</span>
  {/if}
</p>
