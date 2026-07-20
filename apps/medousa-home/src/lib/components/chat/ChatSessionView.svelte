<script lang="ts">
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { LoaderCircle, MessageSquare } from "@lucide/svelte";

  interface Props {
    sessionId: string;
    /** Focused pane — full composer + send. */
    interactive?: boolean;
    visible?: boolean;
    onOpenContext?: () => void;
    onOpenConnection?: () => void;
  }

  let {
    sessionId,
    interactive = true,
    visible = true,
    onOpenContext,
    onOpenConnection,
  }: Props = $props();

  const trimmed = $derived(sessionId.trim());
  /** Use focusedSessionId — raw sessionId swaps during background SSE apply. */
  const isPrincipal = $derived(trimmed === chat.focusedSessionId);
  const messages = $derived(chat.messagesFor(trimmed));
  const loading = $derived(chat.historyLoadingFor(trimmed));
  const error = $derived(chat.streamErrorFor(trimmed));
  // Do not auto switchSession here — shell tab/pane activate owns that. An effect
  // raced openChat(B) while tab A was still mounted and yanked focus back to A.
</script>

{#if interactive && isPrincipal}
  <ChatPanel
    {visible}
    {onOpenContext}
    {onOpenConnection}
  />
{:else}
  <div
    class="chat-session-view-readonly flex h-full min-h-0 flex-col overflow-hidden
      {visible ? '' : 'hidden'}"
    data-debug-label="chat-session-view-readonly"
    data-session-id={trimmed}
  >
    <div class="flex items-center gap-2 border-b border-surface-500/30 px-3 py-2">
      <MessageSquare size={14} strokeWidth={1.75} class="text-surface-500" />
      <p class="workshop-faint text-[11px]">
        Live transcript — focus this pane to type
      </p>
    </div>
    <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-3 py-3">
      {#if loading && messages.length === 0}
        <div class="flex min-h-[120px] items-center justify-center">
          <LoaderCircle size={20} class="animate-spin text-surface-500/80" aria-label="Loading" />
        </div>
      {:else if messages.length === 0}
        <p class="workshop-faint px-2 py-8 text-center text-xs">No messages yet.</p>
      {:else}
        <ChatMessageList
          {messages}
          sessionId={trimmed}
          mobile={false}
        />
      {/if}
      {#if error}
        <p class="mt-2 px-2 text-xs text-error-400">{error}</p>
      {/if}
    </div>
  </div>
{/if}
