<script lang="ts">
  import { onMount } from "svelte";
  import { mediaFetchUrl } from "$lib/daemon";
  import type { ChatMediaAttachment } from "$lib/types/media";

  interface Props {
    sessionId: string;
    attachments: ChatMediaAttachment[];
    compact?: boolean;
  }

  let { sessionId, attachments, compact = false }: Props = $props();

  let urls = $state<Record<string, string>>({});

  onMount(() => {
    let cancelled = false;
    void (async () => {
      const next: Record<string, string> = {};
      for (const attachment of attachments) {
        if (!attachment.mime.startsWith("image/")) continue;
        try {
          next[attachment.mediaId] = await mediaFetchUrl(sessionId, attachment.mediaId);
        } catch {
          // Thumbnail optional — chip still shows label.
        }
      }
      if (!cancelled) urls = next;
    })();
    return () => {
      cancelled = true;
    };
  });
</script>

<div class="chat-media-parts flex flex-wrap gap-2 {compact ? 'mt-1' : 'mt-2'}">
  {#each attachments as attachment (attachment.mediaId)}
    {#if attachment.mime.startsWith("image/") && urls[attachment.mediaId]}
      <figure class="overflow-hidden rounded-lg border border-surface-500/40 bg-surface-900/50">
        <img
          src={urls[attachment.mediaId]}
          alt={attachment.label}
          class="{compact ? 'max-h-28' : 'max-h-40'} max-w-full object-contain"
          loading="lazy"
        />
      </figure>
    {:else}
      <span
        class="inline-flex max-w-full items-center rounded-full border border-surface-500/45 bg-surface-800/80 px-2.5 py-1 text-[11px] text-primary-200"
        title={attachment.mediaId}
      >
        {attachment.label}
      </span>
    {/if}
  {/each}
</div>
