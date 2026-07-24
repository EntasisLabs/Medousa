<script lang="ts">
  import { Paperclip, X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();
</script>

<div class="chat-attachment-bar flex flex-wrap items-center gap-2 border-b border-surface-500/30 bg-surface-900/35 px-3 py-2">
  <button
    type="button"
    class="btn btn-sm variant-soft-surface"
    disabled={disabled || chat.pendingMediaUploading}
    onclick={() => void chat.attachFilesFromPicker()}
  >
    <Paperclip size={14} strokeWidth={2} />
    {chat.pendingMediaUploading ? "Uploading…" : "Attach"}
  </button>

  {#each chat.pendingMediaRefs as attachment (attachment.media_id)}
    <div class="inline-flex max-w-full items-center gap-1 rounded-full border border-surface-500/45 bg-surface-800/80 pl-2.5 pr-1">
      <span class="truncate text-xs text-primary-200" title={attachment.media_id}>
        {attachment.label?.trim() || attachment.media_id}
      </span>
      <button
        type="button"
        class="inline-flex h-6 w-6 items-center justify-center rounded-full text-surface-500 hover:bg-surface-700 hover:text-surface-200"
        aria-label="Remove attachment"
        {disabled}
        onclick={() => chat.removePendingMedia(attachment.media_id)}
      >
        <X size={12} strokeWidth={2} />
      </button>
    </div>
  {/each}
</div>
