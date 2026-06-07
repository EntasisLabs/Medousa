<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    onCanceled: (cardId: string, message: string) => void;
  }

  let { onCanceled }: Props = $props();

  let active = $state(false);
  let busy = $state(false);
  let lastMessage = $state<string | null>(null);

  function allowDrop(event: DragEvent) {
    if (!event.dataTransfer?.types.includes("application/x-medousa-card")) return;
    event.preventDefault();
    active = true;
  }

  function clearActive() {
    active = false;
  }

  async function handleDrop(event: DragEvent) {
    event.preventDefault();
    active = false;
    const cardId = event.dataTransfer?.getData("application/x-medousa-card");
    if (!cardId || busy) return;

    busy = true;
    lastMessage = null;
    try {
      const response = await workspace.cancelCard(cardId);
      lastMessage = response.message;
      onCanceled(cardId, response.message);
    } catch (err) {
      lastMessage = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="mx-4 text-center text-[11px] transition-all {active || busy
    ? 'mb-2 border-b border-dashed px-2 py-1.5'
    : 'mb-0 h-0 overflow-hidden border-0 py-0 opacity-0'} {active
    ? 'border-error-500/60 bg-error-500/5 text-error-300'
    : busy
      ? 'border-surface-500/30 text-surface-400'
      : 'border-surface-500/30 text-surface-500'}"
  role="region"
  aria-label="Cancel work drop zone"
  aria-hidden={!active && !busy}
  ondragover={allowDrop}
  ondragleave={clearActive}
  ondrop={handleDrop}
>
  {#if busy}
    Cancelling work…
  {:else if active}
    Release to cancel this card
  {:else}
    Drag an in-flight card here to cancel
  {/if}
  {#if lastMessage}
    <p class="mt-1 text-xs text-surface-500">{lastMessage}</p>
  {/if}
</div>
