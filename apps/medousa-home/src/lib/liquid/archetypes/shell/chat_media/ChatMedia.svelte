<script lang="ts">
  /** `chat_media` shell archetype — reuses the media attachment strip. */
  import ChatMediaParts from "$lib/components/chat/ChatMediaParts.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import type { ChatMediaAttachment } from "$lib/types/media";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const attachments = $derived(
    Array.isArray(node.props.attachments) ? (node.props.attachments as ChatMediaAttachment[]) : [],
  );
</script>

{#if attachments.length > 0 && ctx.sessionId}
  <ChatMediaParts
    sessionId={ctx.sessionId}
    {attachments}
    compact={(ctx.mobile ?? false) || (ctx.compact ?? false)}
  />
{/if}
