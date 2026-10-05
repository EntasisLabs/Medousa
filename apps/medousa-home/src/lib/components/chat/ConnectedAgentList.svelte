<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { Plus, RefreshCw } from "@lucide/svelte";
  import { type ExternalConversation, type ExternalProvider } from "$lib/daemon/externalConversations";
  import { externalConversationBinding, externalConversationSessionId } from "$lib/utils/externalConversationSession";
  import { getExternalConversationSelection } from "$lib/utils/externalConversationSelection";
  import { agentRuntimeLabel, getSessionAgentRuntime } from "$lib/utils/sessionAgentRuntime";
  import { connectedAgents } from "$lib/stores/connectedAgents.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { agentCreation } from "$lib/stores/agentCreation.svelte";

  let { open, query, onSelect }: { open: boolean; query: string; onSelect: (sessionId: string, title: string) => void } = $props();
  const conversations = $derived(connectedAgents.workshopScopeId === chat.workshopScopeId ? connectedAgents.conversations : []);
  const loading = $derived(connectedAgents.loading);
  const error = $derived(connectedAgents.error);
  const providers: ExternalProvider[] = ["grok_bot", "muse", "instinct", "dots"];
  const groups = $derived(providers.map((provider) => ({
    provider,
    label: agentRuntimeLabel(provider),
    conversations: conversations.filter((item) => item.provider === provider &&
      `${item.label} ${agentRuntimeLabel(provider)}`.toLowerCase().includes(query.trim().toLowerCase()))
      .sort((a, b) => a.label.localeCompare(b.label)),
  })).filter((group) => group.conversations.length > 0));
  const current = $derived(externalConversationBinding(chat.focusedSessionId));

  async function refresh() { await connectedAgents.refresh(chat.workshopScopeId ?? "", true); }
  $effect(() => {
    const scope = chat.workshopScopeId;
    if (open && !connection.offline) untrack(() => {void connectedAgents.refresh(scope ?? "");});
  });
  onMount(() => {
    const changed = () => {if (open && !connection.offline) void refresh();};
    window.addEventListener("medousa-external-conversation-changed",changed);
    return () => window.removeEventListener("medousa-external-conversation-changed",changed);
  });
  function selected(item: ExternalConversation): boolean {
    if (current) return current.provider === item.provider && current.id === item.id;
    return getSessionAgentRuntime(chat.focusedSessionId) === item.provider && getExternalConversationSelection(chat.focusedSessionId, item.provider) === item.id;
  }
  function manage() { agentCreation.connectAgent(); }
</script>

<li class="session-sidebar-section">
  <div class="session-sidebar-section-heading">
    <p class="session-sidebar-section-title">Connected agents</p>
    <span class="session-sidebar-section-heading__trailing">
      <button type="button" class="session-sidebar-heading-action" aria-label="Refresh connected agents" title="Refresh connected agents" disabled={loading || connection.offline} onclick={() => void refresh()}><RefreshCw size={12} class={loading ? "animate-spin" : ""} /></button>
      <button type="button" class="session-sidebar-heading-action" aria-label="Connect an agent" title="Connect an agent" onclick={manage}><Plus size={13} /></button>
    </span>
  </div>
  {#each groups as group (group.provider)}
    <ul class="session-sidebar-section-list" aria-label={group.label}>
      {#each group.conversations as item (item.id)}
        <li class="session-row" class:session-row--selected={selected(item)}>
          <button type="button" class="session-row-main bot-row-main" onclick={() => onSelect(externalConversationSessionId(item.provider, item.id), item.label)}>
            <span class="agent-avatar" aria-hidden="true">{item.label.slice(0, 1).toUpperCase()}</span>
            <span class="min-w-0 flex-1"><span class="session-row-title">{item.label}</span><span class="bot-row-specialist">{group.label}</span></span>
          </button>
        </li>
      {/each}
    </ul>
  {/each}
  {#if error}<p role="alert" class="px-4 py-2 text-xs text-content-error">{error}</p>
  {:else if loading && conversations.length === 0}<p class="workshop-faint px-4 py-2 text-[11px]">Loading connected agents…</p>
  {:else if connection.offline}<p class="workshop-faint px-4 py-2 text-[11px]">Connect to your workshop to open its agents.</p>
  {:else if groups.length === 0 && !query.trim()}<div class="session-row"><button type="button" class="session-row-main" onclick={manage}><span class="session-row-title">Connect an agent</span></button></div>{/if}
</li>

<style>
  .agent-avatar { display: grid; place-items: center; flex: 0 0 auto; width: 30px; height: 30px; border-radius: 28%; background: rgb(var(--theme-border) / .25); color: rgb(var(--theme-text-secondary)); font-size: 13px; font-weight: 600; }
</style>
