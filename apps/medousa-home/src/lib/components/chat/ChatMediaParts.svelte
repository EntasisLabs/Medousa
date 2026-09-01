<script lang="ts">
  import { onMount } from "svelte";
  import { X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { readMediaBytes } from "$lib/daemon";
  import type { ChatMediaAttachment } from "$lib/types/media";
  import { chatImageObjectUrl } from "$lib/utils/chatImagePreview";

  interface Props {
    sessionId: string;
    attachments: ChatMediaAttachment[];
    compact?: boolean;
  }

  let { sessionId, attachments, compact = false }: Props = $props();

  let urls = $state<Record<string, string>>({});
  let preview = $state<{ url: string; label: string } | null>(null);

  $effect(() => {
    const activeSessionId = sessionId;
    const images = attachments
      .filter((attachment) => attachment.mime.startsWith("image/"))
      .map((attachment) => ({ ...attachment }));
    let cancelled = false;
    let ownedUrls: string[] = [];
    urls = {};
    preview = null;
    void (async () => {
      const next: Record<string, string> = {};
      const created: string[] = [];
      for (const attachment of images) {
        try {
          const payload = await readMediaBytes(activeSessionId, attachment.mediaId);
          const url = await chatImageObjectUrl(
            payload.bytes,
            payload.mime,
            attachment.label,
          );
          created.push(url);
          next[attachment.mediaId] = url;
        } catch {
          // Thumbnail optional — chip still shows label.
        }
      }
      if (cancelled) {
        created.forEach((url) => URL.revokeObjectURL(url));
        return;
      }
      ownedUrls = created;
      urls = next;
    })();
    return () => {
      cancelled = true;
      ownedUrls.forEach((url) => URL.revokeObjectURL(url));
    };
  });

  onMount(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") preview = null;
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  });

  function closeFromBackdrop(event: MouseEvent) {
    if (event.target === event.currentTarget) preview = null;
  }
</script>

<div class="chat-media-parts {compact ? 'chat-media-parts-compact' : ''}">
  {#each attachments as attachment (attachment.mediaId)}
    {#if attachment.mime.startsWith("image/") && urls[attachment.mediaId]}
      <button
        type="button"
        class="chat-media-thumbnail"
        aria-label="Open {attachment.label}"
        title={attachment.label}
        onclick={() => (preview = { url: urls[attachment.mediaId], label: attachment.label })}
      >
        <img
          src={urls[attachment.mediaId]}
          alt={attachment.label}
          class="chat-media-thumbnail-image"
          loading="lazy"
        />
        <span class="chat-media-thumbnail-shine" aria-hidden="true"></span>
      </button>
    {:else}
      <span class="chat-media-file" title={attachment.mediaId}>
        {attachment.label}
      </span>
    {/if}
  {/each}
</div>

{#if preview}
  <BodyPortal>
    <div
      class="chat-media-preview-backdrop"
      role="presentation"
      onclick={closeFromBackdrop}
    >
      <div
        class="chat-media-preview-dialog"
        role="dialog"
        tabindex="-1"
        aria-modal="true"
        aria-label={preview.label}
      >
        <button
          type="button"
          class="chat-media-preview-close"
          aria-label="Close image preview"
          onclick={() => (preview = null)}
        >
          <X size={17} strokeWidth={2} />
        </button>
        <img src={preview.url} alt={preview.label} class="chat-media-preview-image" />
        <div class="chat-media-preview-label">{preview.label}</div>
      </div>
    </div>
  </BodyPortal>
{/if}
