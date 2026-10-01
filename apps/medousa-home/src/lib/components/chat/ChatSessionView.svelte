<script lang="ts">
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import SessionIdentity from "./SessionIdentity.svelte";
  import ExternalConversationTranscript from "./ExternalConversationTranscript.svelte";
  import { createExternalConversationController } from "$lib/chat/externalConversationController.svelte";
  import { getSessionAgentRuntime, isProviderConversationRuntime } from "$lib/utils/sessionAgentRuntime";
  import { connection } from "$lib/stores/connection.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import "$lib/components/ui/bindMarkdownView";
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
  const runtime = $derived(getSessionAgentRuntime(trimmed));
  const provider = $derived(isProviderConversationRuntime(runtime) ? runtime : null);
  const external = createExternalConversationController({
    provider: () => interactive && isPrincipal ? null : provider,
    sessionId: () => trimmed, scope: () => chat.workshopScopeId,
    offline: () => connection.offline, visible: () => visible,
  });
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
      {#if provider || bots.forSession(trimmed)}<SessionIdentity sessionId={trimmed} conversation={external.selected} {provider} />{/if}
      <MessageSquare size={14} strokeWidth={1.75} class="text-content-quiet" />
      <p class="workshop-faint text-[11px]">
        Live transcript — focus this pane to type
      </p>
    </div>
    <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-3 py-3">
      {#if provider}
        <ExternalConversationTranscript {provider} selected={external.selected} choices={external.choices} messages={external.messages} sessionId={trimmed} mobile={false} loading={external.loading} error={external.error} />
      {:else if loading && messages.length === 0}
        <div class="flex min-h-[120px] items-center justify-center">
          <LoaderCircle size={20} class="animate-spin text-content-quiet/80" aria-label="Loading" />
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
      {#if !provider && error}
        <p class="mt-2 px-2 text-xs text-content-error">{error}</p>
      {/if}
    </div>
  </div>
{/if}
