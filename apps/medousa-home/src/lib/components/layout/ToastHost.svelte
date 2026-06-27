<script lang="ts">
  import { toast } from "$lib/stores/toast.svelte";
</script>

{#if toast.message}
  <div
    class="pointer-events-auto fixed bottom-10 left-1/2 z-[100] flex -translate-x-1/2 items-center gap-2 rounded-full border border-surface-500/50 bg-surface-900/95 px-4 py-2 text-sm text-surface-100 shadow-xl"
    role="status"
  >
    <span>{toast.message}</span>
    {#if toast.action}
      <button
        type="button"
        class="text-primary-300 hover:text-primary-200"
        onclick={() => {
          toast.action?.onAction();
          toast.dismiss();
        }}
      >
        {toast.action.label}
      </button>
    {/if}
  </div>
{/if}
