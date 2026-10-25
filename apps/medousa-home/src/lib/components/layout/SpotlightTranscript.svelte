<script lang="ts">
  import { tick, untrack } from "svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import ChatMediaParts from "$lib/components/chat/ChatMediaParts.svelte";
  import { friendlyUserError } from "$lib/utils/normieErrors";
  import { ArrowUpRight } from "@lucide/svelte";
  import type { AddressedConversation } from "$lib/spotlight/addressedConversation.svelte";
  let { conversation, onOpen }: { conversation: AddressedConversation; onOpen: () => void | Promise<void> } = $props();
  let transcript: HTMLDivElement;
  let following = true;
  let openError = $state<string | null>(null);
  const messages = $derived(conversation.messages.filter(message => message.content.trim() || message.mediaAttachments?.length));
  const failure = $derived(conversation.error ?? openError ?? conversation.failure);
  $effect(() => { void conversation.sessionId; openError = null; following = true; });
  async function open() {
    openError = null;
    try { await onOpen(); }
    catch (cause) { openError = cause instanceof Error ? cause.message : String(cause); }
  }
  $effect(() => {
    void messages.map(message => `${message.id}:${message.content}`).join();
    void conversation.working;
    if (untrack(() => following)) void tick().then(() => { if (following && transcript) transcript.scrollTop = transcript.scrollHeight; });
  });
</script>

<div class="command-spotlight-conversation">
  <div class="command-spotlight-transcript" role="log" aria-label="Recent conversation" bind:this={transcript}
    onscroll={() => { following = transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 48; }}>
    {#each messages as message (message.id)}
      <div class="command-spotlight-transcript-entry" class:command-spotlight-transcript-user={message.role === "user"}>
        <span class="command-spotlight-transcript-speaker">{message.role === "user" ? "You" : conversation.assistantLabel}</span>
        {#if message.role === "assistant"}<MarkdownContent content={message.content} streaming={Boolean(message.streaming)}/>{:else}<p>{message.content}</p>{/if}
        {#if conversation.sessionId && message.mediaAttachments?.length}<ChatMediaParts sessionId={conversation.sessionId} attachments={message.mediaAttachments} compact/>{/if}
      </div>
    {:else}
      {#if !conversation.sending && !conversation.working}<p class="command-spotlight-conversation-empty">{conversation.loading ? "Loading conversation…" : "What would you like to work on?"}</p>{/if}
    {/each}
    {#if conversation.sending || conversation.working}
      <span class="command-spotlight-transcript-status" role="status"><span class="command-spotlight-working-dot" aria-hidden="true"></span>{conversation.sending ? "Sending…" : "Working…"}</span>
    {/if}
  </div>
  {#if failure}
    <details class="command-spotlight-conversation-failure">
      <summary>{conversation.error ? "Couldn’t send this message" : openError ? "Couldn’t open this conversation" : "This request couldn’t finish"}</summary>
      <p>{friendlyUserError(failure)}</p>
    </details>
  {/if}
  {#if conversation.sessionId}<button class="command-spotlight-open-conversation" type="button" onclick={open}>Open conversation <ArrowUpRight size={13}/></button>{/if}
</div>
