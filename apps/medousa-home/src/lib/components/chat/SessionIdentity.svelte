<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronDown, Pencil, X } from "@lucide/svelte";
  import BotAvatar from "./BotAvatar.svelte";
  import BotEditor from "./BotEditor.svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { externalConversationBinding } from "$lib/utils/externalConversationSession";
  import { agentRuntimeLabel, getSessionAgentRuntime, isProviderConversationRuntime } from "$lib/utils/sessionAgentRuntime";
  import { getExternalConversationSelection } from "$lib/utils/externalConversationSelection";
  import { getExternalConversation, type ExternalConversation, type ExternalProvider } from "$lib/daemon/externalConversations";
  import type { BotProfile, BotWorldBinding } from "$lib/types/generated/daemon_api";

  let { sessionId, conversation, provider = null }: {
    sessionId: string; conversation?: ExternalConversation | null; provider?: ExternalProvider | null;
  } = $props();
  let loaded = $state<ExternalConversation | null>(null);
  let open = $state(false);
  let editing = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let draftBot = $state<BotProfile | null>(null);
  let name = $state("");
  let purpose = $state("");
  let avatar = $state("");
  let archetypeId = $state("");
  let worldBinding = $state<BotWorldBinding | null>(null);
  const bot = $derived(bots.forSession(sessionId));
  const binding = $derived(externalConversationBinding(sessionId));
  const external = $derived(conversation === undefined ? loaded : conversation);
  const runtime = $derived(getSessionAgentRuntime(sessionId));
  const externalProvider = $derived(provider ?? binding?.provider ?? (isProviderConversationRuntime(runtime) ? runtime : null));
  const label = $derived(bot?.display_name ?? external?.label ?? (externalProvider ? agentRuntimeLabel(externalProvider) : "Medousa"));
  const archetype = $derived(catalog.manuscripts.find((entry) => entry.id === bot?.primary_manuscript_id)?.name ?? "");

  onMount(() => { void bots.refresh().catch(() => undefined); });
  $effect(() => {
    const id = externalProvider ? getExternalConversationSelection(sessionId, externalProvider) : null;
    chat.workshopScopeId;
    loaded = null;
    let cancelled = false;
    if (conversation === undefined && id) void getExternalConversation(id).then((value) => { if (!cancelled) loaded = value; }).catch(() => undefined);
    return () => { cancelled = true; };
  });
  $effect(() => { sessionId; chat.workshopScopeId; open = false; editing = false; error = null; });

  function showProfile() {
    open = true;
    if (catalog.manuscripts.length === 0 && !catalog.loading) void catalog.refresh();
  }
  function edit() {
    if (!bot) return;
    draftBot = bot;
    name = bot.display_name;
    purpose = bot.role_description ?? "";
    avatar = bot.avatar_ref ?? "";
    archetypeId = bot.primary_manuscript_id;
    worldBinding = bot.world_binding ? { ...bot.world_binding } : null;
    error = null;
    open = false;
    editing = true;
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!draftBot || saving) return;
    const scope = chat.workshopScopeId;
    const id = sessionId;
    saving = true;
    try {
      await bots.update(draftBot, { display_name: name.trim(), role_description: purpose.trim(), avatar_ref: avatar,
        primary_manuscript_id: archetypeId, world_binding: worldBinding, clear_world_binding: !worldBinding });
      if (scope === chat.workshopScopeId && id === sessionId) {
        editing = false;
        void chat.refreshSessions({ force: true });
      }
    } catch (cause) {
      if (scope === chat.workshopScopeId && id === sessionId) error = cause instanceof Error ? cause.message : String(cause);
    } finally { saving = false; }
  }
  function mountDialog(node: HTMLDialogElement) {
    node.showModal();
    const unregister = registerMobileBackHandler(() => { open = false; return true; }, "modal");
    return { destroy() { unregister(); node.close(); } };
  }
  function manage() {
    open = false;
    settingsNav.setActiveSection("connections");
    if (layout.isMobile) layout.openMore("settings");
    else layout.navigateDesktop("settings");
  }
</script>

<button type="button" class="session-identity" aria-label={`View ${label} profile`} aria-haspopup="dialog" aria-expanded={open} onclick={showProfile}>
  {#if bot}<BotAvatar reference={bot.avatar_ref} size={28} />{:else}<span class="agent-avatar" aria-hidden="true">{label.slice(0, 1).toUpperCase()}</span>{/if}
  <span class="identity-name">{label}</span><ChevronDown size={12} class="shrink-0 opacity-50" />
</button>

{#if open}
  <BodyPortal>
    <dialog use:mountDialog class="identity-dialog" aria-label={`${label} profile`} onkeydown={(event) => { if (event.key === "Escape") event.stopPropagation(); }} oncancel={(event) => { event.preventDefault(); open = false; }} onclick={(event) => { if (event.target === event.currentTarget) open = false; }}>
      <div class="profile">
        <button type="button" class="close" aria-label="Close profile" onclick={() => open = false}><X size={18} /></button>
        {#if bot}<BotAvatar reference={bot.avatar_ref} size={64} />{:else}<span class="agent-avatar large" aria-hidden="true">{label.slice(0, 1).toUpperCase()}</span>{/if}
        <h2>{label}</h2>
        <p class="archetype">{bot ? archetype || "Bot" : externalProvider ? agentRuntimeLabel(externalProvider) : "Connected agent"}</p>
        {#if bot?.role_description}<p class="purpose">{bot.role_description}</p>{/if}
        <p class="memory">{bot ? "This conversation and its memory stay with this Bot." : "This conversation stays with the connected agent on your workshop."}</p>
        <button type="button" class="edit" onclick={bot ? edit : manage}><Pencil size={15} />{bot ? "Edit Bot" : "Manage connection"}</button>
      </div>
    </dialog>
  </BodyPortal>
{/if}
{#if editing}
  <BotEditor bind:name bind:purpose bind:avatar bind:archetypeId bind:worldBinding editing saving={saving} {error} onclose={() => { if (!saving) editing = false; }} onsubmit={(event) => void save(event)} />
{/if}

<style>
  .session-identity { display: inline-flex; align-items: center; gap: 9px; min-width: 0; max-width: 100%; padding: 4px 7px; border-radius: 10px; color: rgb(var(--theme-text-primary)); transition: background .15s; }
  .session-identity:hover { background: rgb(var(--theme-border) / .22); }
  .identity-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 14px; font-weight: 600; }
  .agent-avatar { display: grid; place-items: center; flex: 0 0 auto; width: 28px; height: 28px; border-radius: 28%; background: rgb(var(--theme-border) / .25); color: rgb(var(--theme-text-secondary)); font-size: 12px; }
  .agent-avatar.large { width: 64px; height: 64px; font-size: 24px; }
  .identity-dialog { padding: 0; width: min(360px, calc(100vw - 40px)); max-width: none; border: 1px solid rgb(var(--theme-border) / .65); border-radius: 22px; background: rgb(var(--theme-card, 25 23 34)); color: rgb(var(--theme-text-primary)); box-shadow: 0 24px 80px #0007; }
  .identity-dialog::backdrop { background: #09081088; backdrop-filter: blur(4px); }
  .profile { position: relative; display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 30px 24px 24px; text-align: center; }
  .close { position: absolute; top: 12px; right: 12px; padding: 5px; color: rgb(var(--theme-text-secondary)); }
  h2 { margin: 4px 0 0; font-size: 20px; font-weight: 600; overflow-wrap: anywhere; }
  .archetype, .memory { font-size: 12px; color: rgb(var(--theme-text-tertiary)); }
  .purpose { margin-top: 8px; font-size: 14px; line-height: 1.6; overflow-wrap: anywhere; }
  .memory { margin-top: 8px; line-height: 1.5; }
  .edit { display: flex; align-items: center; justify-content: center; gap: 8px; width: 100%; min-height: 42px; margin-top: 8px; border: 1px solid rgb(var(--theme-border) / .5); border-radius: 12px; font-size: 13px; }
  .edit:hover { background: rgb(var(--theme-border) / .2); }
</style>
