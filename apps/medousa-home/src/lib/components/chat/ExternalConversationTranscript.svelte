<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import { externalConversationStatus } from "$lib/utils/externalConversationMessages";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import type { ExternalConversation, ExternalProvider } from "$lib/daemon/externalConversations";
  import type { ChatMessage } from "$lib/types/chat";

  let {
    provider,
    selected,
    choices,
    messages,
    sessionId,
    mobile,
    loading,
    error,
  }: {
    provider: ExternalProvider;
    selected: ExternalConversation | null;
    choices: ExternalConversation[];
    messages: ChatMessage[];
    sessionId: string;
    mobile: boolean;
    loading: boolean;
    error: string | null;
  } = $props();

  function openSettings() {
    settingsNav.setActiveSection("connections");
    if (layout.isMobile) layout.openMore("settings");
    else layout.navigateDesktop("settings");
  }
</script>

{#if error}<p role="alert" class="px-2 text-sm text-content-error">{error}</p>{/if}
{#if selected}
  {#if externalConversationStatus(selected)}
    <p class="workshop-faint px-2 text-xs">{externalConversationStatus(selected)}</p>
  {/if}
  {#if messages.length > 0}
    <ChatMessageList {messages} {sessionId} {mobile} />
  {:else}
    <p class="workshop-faint px-2 py-8 text-sm">Message {selected.label} to start this conversation.</p>
  {/if}
{:else if loading}
  <div class="flex min-h-[160px] items-center justify-center"><LoaderCircle size={22} class="animate-spin text-content-quiet/80" aria-label="Loading sessions" /></div>
{:else}
  <div class="px-2 py-8 text-sm">
    <p class="text-surface-200">{choices.length > 0 ? (mobile ? "Choose a session or bot in Chat context → Runtime." : "Choose a session or bot from the model picker.") : "No " + (provider === "muse" ? "Muse sessions" : provider === "instinct" ? "Instinct agents" : provider === "dots" ? "dots" : "Grok bots") + " connected yet."}</p>
    {#if choices.length === 0}<button type="button" class="mt-3 text-content-link" onclick={openSettings}>Set up in External Agents</button>{/if}
  </div>
{/if}
