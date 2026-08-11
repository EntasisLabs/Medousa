<script lang="ts">
  import { X } from "@lucide/svelte";
  import { mediaFetchUrl } from "$lib/daemon";
  import { chat } from "$lib/stores/chat.svelte";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();
  let urls = $state<Record<string, string>>({});

  $effect(() => {
    const pendingImages = chat.pendingMediaRefs.filter(
      (attachment) =>
        attachment.mime.startsWith("image/") && !urls[attachment.media_id],
    );
    if (pendingImages.length === 0) return;
    let cancelled = false;
    void (async () => {
      const loaded: Record<string, string> = {};
      for (const attachment of pendingImages) {
        try {
          loaded[attachment.media_id] = await mediaFetchUrl(
            chat.sessionId,
            attachment.media_id,
          );
        } catch {
          // Keep the filename chip when a preview is unavailable.
        }
      }
      if (!cancelled) urls = { ...urls, ...loaded };
    })();
    return () => {
      cancelled = true;
    };
  });
</script>

{#if chat.pendingMediaRefs.length > 0}
  <div class="composer-attachment-chips">
    {#each chat.pendingMediaRefs as attachment (attachment.media_id)}
      {@const label = attachment.label?.trim() || attachment.media_id}
      {#if attachment.mime.startsWith("image/") && urls[attachment.media_id]}
        <figure class="composer-attachment-preview" title={label}>
          <img
            src={urls[attachment.media_id]}
            alt={label}
            class="composer-attachment-preview-image"
          />
          <button
            type="button"
            class="composer-attachment-preview-remove"
            aria-label="Remove {label}"
            {disabled}
            onclick={() => chat.removePendingMedia(attachment.media_id)}
          >
            <X size={12} strokeWidth={2.25} />
          </button>
          <figcaption class="composer-attachment-preview-label">{label}</figcaption>
        </figure>
      {:else}
        <div class="composer-attachment-chip">
          <span class="truncate" title={attachment.media_id}>{label}</span>
          <button
            type="button"
            class="composer-attachment-chip-remove"
            aria-label="Remove {label}"
            {disabled}
            onclick={() => chat.removePendingMedia(attachment.media_id)}
          >
            <X size={12} strokeWidth={2} />
          </button>
        </div>
      {/if}
    {/each}
  </div>
{/if}
