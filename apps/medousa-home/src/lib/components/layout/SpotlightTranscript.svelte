<script lang="ts">
  import { ArrowUpRight } from "@lucide/svelte";
  import type { AddressedConversation } from "$lib/spotlight/addressedConversation.svelte";
  let { conversation, onOpen }: { conversation: AddressedConversation; onOpen: () => void } = $props();
  let transcript: HTMLDivElement;
  $effect(() => {
    void conversation.messages.map(message => `${message.id}:${message.content}:${message.statusLine}`).join();
    if (transcript) transcript.scrollTop = transcript.scrollHeight;
  });
</script>

<div class="command-spotlight-conversation">
  <div class="command-spotlight-transcript" role="log" aria-label="Recent conversation" bind:this={transcript}>
    {#each conversation.messages as message (message.id)}
      <div class="command-spotlight-transcript-entry">
        {#if message.content}
          <span class="command-spotlight-transcript-speaker">{message.role === "user" ? "You" : conversation.assistantLabel}</span>
          <p>{message.content}</p>
          {#if message.streaming && message.statusLine}<span class="command-spotlight-transcript-status">{message.statusLine}</span>{/if}
        {:else}
          <span class="command-spotlight-transcript-status" role="status">{message.statusLine || "Working…"}</span>
        {/if}
      </div>
    {:else}
      <p class="command-spotlight-conversation-empty">{conversation.loading ? "Loading recent conversation…" : conversation.sessionId ? "Send a message to continue this conversation." : `Send a message to start with ${conversation.target?.label}.`}</p>
    {/each}
  </div>
  {#if conversation.error}<p class="command-spotlight-conversation-notice" role="alert">{conversation.error}</p>
  {:else if conversation.sending || conversation.status}<p class="command-spotlight-conversation-notice" role="status">{conversation.sending ? "Sending…" : conversation.status}</p>{/if}
  {#if conversation.sessionId}<button class="command-spotlight-open-conversation" type="button" onclick={onOpen}>Open conversation <ArrowUpRight size={13}/></button>{/if}
</div>
