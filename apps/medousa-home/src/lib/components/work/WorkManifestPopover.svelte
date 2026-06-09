<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { X } from "@lucide/svelte";
  import CardInspector from "$lib/components/work/CardInspector.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
  }

  let { onOpenNote, onOpenChat }: Props = $props();

  function close() {
    workspace.clearSelection();
  }

  function handleBackdropKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") close();
  }
</script>

{#if workspace.selectedCardId}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="work-manifest-backdrop"
    role="presentation"
    transition:fade={{ duration: 180 }}
    onclick={close}
    onkeydown={handleBackdropKeydown}
  ></div>

  <div
    class="work-manifest-popover"
    role="dialog"
    aria-modal="true"
    aria-label="Work manifestation"
    transition:fly={{ y: 28, duration: 240, opacity: 0.98 }}
  >
    <button
      type="button"
      class="work-manifest-close"
      aria-label="Close"
      onclick={close}
    >
      <X size={18} strokeWidth={1.75} />
    </button>
    <CardInspector
      popover={true}
      {onOpenNote}
      {onOpenChat}
      onClose={close}
    />
  </div>
{/if}
