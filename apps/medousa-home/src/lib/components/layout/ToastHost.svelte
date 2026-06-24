<script lang="ts">
  import { toast } from "$lib/stores/toast.svelte";
</script>

{#if toast.message}
  <div class="pointer-events-none fixed bottom-10 left-1/2 z-[100] -translate-x-1/2" role="status">
    <p class="rounded-full border border-surface-500/50 bg-surface-900/95 px-4 py-2 text-sm text-surface-100 shadow-xl">
      {toast.message}
    </p>
  </div>
{/if}
