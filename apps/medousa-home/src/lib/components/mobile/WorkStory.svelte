<script lang="ts">
  import CardInspector from "$lib/components/work/CardInspector.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
    onClose: () => void;
  }

  let { onOpenNote, onOpenChat, onClose }: Props = $props();
</script>

{#if workspace.selectedCardId}
  <div class="mobile-story-overlay">
    <header class="mobile-story-header">
      <button type="button" class="workshop-text-action text-sm" onclick={onClose}>
        ← Back
      </button>
    </header>
    <div class="min-h-0 flex-1 overflow-y-auto">
      <CardInspector {onOpenNote} {onOpenChat} onClose={onClose} split={false} />
    </div>
  </div>
{/if}
