<script lang="ts">
  import { pushBrowserPopoverOverlay, popBrowserPopoverOverlay } from "$lib/utils/browserPopoverOverlay";
  import { onMount } from "svelte";
  import { X } from "@lucide/svelte";
  import BotEditor from "./BotEditor.svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import ExternalConversationSettingsCard from "$lib/components/settings/ExternalConversationSettingsCard.svelte";
  import { connectedAgents } from "$lib/stores/connectedAgents.svelte";
  import { agentCreation } from "$lib/stores/agentCreation.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { DEFAULT_BOT_AVATAR } from "$lib/utils/botAvatar";
  import { externalConversationSessionId } from "$lib/utils/externalConversationSession";
  import { registerMobileBackHandler, switchMobileTab } from "$lib/mobileNavigation";
  import type { BotWorldBinding, ExternalAgentExecutor } from "$lib/types/generated/daemon_api";
  import type { ExternalConversation, ExternalProvider } from "$lib/daemon/externalConversations";
  let name = $state(""); let purpose = $state(""); let avatar = $state<string>(DEFAULT_BOT_AVATAR);
  let archetypeId = $state(""); let worldBinding = $state<BotWorldBinding | null>(null);
  let externalAgent = $state<ExternalAgentExecutor | null>(null);
  let createdConnection = $state<ExternalConversation | null>(null);
  let saving = $state(false); let error = $state<string | null>(null);
  const scope = chat.workshopScopeId;
  const providers: { id: ExternalProvider; label: string }[] = [{id:"grok_bot",label:"Grok Bot"},{id:"muse",label:"Muse"},{id:"instinct",label:"Instinct"},{id:"dots",label:"Dots"}];
  onMount(() => { void pushBrowserPopoverOverlay(); if (agentCreation.kind === "bot" && catalog.manuscripts.length === 0 && !catalog.loading) void catalog.refresh().catch(() => undefined); return () => {void popBrowserPopoverOverlay();}; });
  $effect(() => { if (chat.workshopScopeId !== scope) agentCreation.close(); });
  async function openSession(id: string, title: string) {
    agentCreation.close(); layout.setSessionDrawerOpen(false);
    if (layout.isMobile) switchMobileTab("chat");
    const tab = shellTabs.openChat(id, { title, activate: true });
    if (!tab) await chat.switchSession(id);
  }
  async function create(event: SubmitEvent) {
    event.preventDefault(); if (saving) return;
    saving = true; error = null;
    try {
      const result = await bots.create({display_name:name.trim(),role_description:purpose.trim(),avatar_ref:avatar,primary_manuscript_id:archetypeId,world_binding:worldBinding,external_agent:externalAgent});
      if (chat.workshopScopeId !== scope) return;
      await chat.refreshSessions({force:true});
      if (chat.workshopScopeId === scope) await openSession(result.binding.session_id,result.bot.display_name);
    } catch (cause) { if (chat.workshopScopeId === scope) error = cause instanceof Error ? cause.message : String(cause); }
    finally { saving = false; }
  }
  function connected(conversation: ExternalConversation) {
    if (chat.workshopScopeId !== scope) return;
    createdConnection = conversation;
    void connectedAgents.refresh(scope ?? "", true);
  }
  function mountDialog(node: HTMLDialogElement) {
    node.showModal(); const unregister = registerMobileBackHandler(() => { agentCreation.close(); return true; }, "modal");
    return { destroy() { unregister(); node.close(); } };
  }
</script>
{#if agentCreation.kind === "bot"}
  <BotEditor bind:name bind:purpose bind:avatar bind:archetypeId bind:worldBinding bind:externalAgent {saving} {error} onclose={() => {if (!saving) agentCreation.close();}} onsubmit={(event) => void create(event)} />
{:else if agentCreation.kind === "connection"}
  <BodyPortal><dialog use:mountDialog aria-label="Connect an agent" onkeydown={(event) => {if(event.key === "Escape") event.stopPropagation();}} oncancel={(event) => {event.preventDefault();agentCreation.close();}}>
    <header><h2>Connect an agent</h2><button type="button" aria-label="Close agent setup" onclick={() => agentCreation.close()}><X size={18}/></button></header>
    <p>Connect a conversation from your agent’s service to this workshop.</p>
    <label>Agent<select aria-label="Agent provider" value={agentCreation.provider ?? ""} onchange={(event) => {createdConnection = null; agentCreation.provider = event.currentTarget.value as ExternalProvider || null;}}><option value="">Choose an agent</option>{#each providers as provider}<option value={provider.id}>{provider.label}</option>{/each}</select></label>
    {#if agentCreation.provider}{#key agentCreation.provider}<ExternalConversationSettingsCard provider={agentCreation.provider} oncreated={connected} />{/key}{/if}
    {#if createdConnection}
      <div class="created-connection">
        <p>Connection saved. Finish setup using the details above, then open your agent’s conversation.</p>
        <button type="button" class="open-connection" onclick={() => {if (createdConnection) void openSession(externalConversationSessionId(createdConnection.provider,createdConnection.id),createdConnection.label);}}>Open chat with {createdConnection.label}</button>
      </div>
    {/if}
  </dialog></BodyPortal>
{/if}
<style>
  dialog { padding:24px; width:min(560px,calc(100vw - 32px)); max-width:none; max-height:calc(100dvh - 48px); overflow-y:auto; border:1px solid rgb(var(--theme-border) / .6); border-radius:22px; background:rgb(var(--theme-card)); color:rgb(var(--theme-text-primary)); }
  dialog::backdrop { background:#09081099; backdrop-filter:blur(4px); }
  header {display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:12px;} h2{font-size:17px;font-weight:600;}
  p{font-size:12px;color:rgb(var(--theme-text-secondary));line-height:1.5;margin-bottom:20px;} label{display:block;font-size:12px;}
  select{display:block;width:100%;padding:12px;margin:8px 0 20px;border-radius:10px;background:rgb(var(--theme-card));border:1px solid rgb(var(--theme-border) / .5);}
  .created-connection{margin-top:20px;} .open-connection{width:100%;padding:12px;border-radius:10px;background:rgb(var(--theme-accent));color:rgb(var(--theme-on-accent,255 255 255));font-size:13px;font-weight:600;}
  @media(max-width:640px){dialog{inset:auto 0 0;width:100%;max-height:calc(100dvh - 28px);margin:0;border-radius:24px 24px 0 0;padding-bottom:calc(24px + env(safe-area-inset-bottom));}}
</style>
