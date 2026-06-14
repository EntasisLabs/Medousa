<script lang="ts">
  import { X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();
</script>

{#if chat.pendingMediaRefs.length > 0}
  <div class="composer-attachment-chips">
    {#each chat.pendingMediaRefs as attachment (attachment.media_id)}
      <div class="composer-attachment-chip">
        <span class="truncate" title={attachment.media_id}>
          {attachment.label?.trim() || attachment.media_id}
        </span>
        <button
          type="button"
          class="composer-attachment-chip-remove"
          aria-label="Remove attachment"
          {disabled}
          onclick={() => chat.removePendingMedia(attachment.media_id)}
        >
          <X size={12} strokeWidth={2} />
        </button>
      </div>
    {/each}
  </div>
{/if}
