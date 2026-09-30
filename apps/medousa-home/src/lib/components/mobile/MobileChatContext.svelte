<script lang="ts">
  import { Check, ChevronDown, ChevronRight, LoaderCircle } from "@lucide/svelte";
  import MobileActionSheet from "./MobileActionSheet.svelte";
  import ChatAgentModePicker from "$lib/components/chat/ChatAgentModePicker.svelte";
  import ChatExecutionTargetPicker from "$lib/components/chat/ChatExecutionTargetPicker.svelte";
  import ChatRuntimePicker from "$lib/components/chat/ChatRuntimePicker.svelte";
  import UndertakingContextChip from "$lib/components/work/UndertakingContextChip.svelte";
  import type { createAgentSessionController } from "$lib/chat/agentSessionController.svelte";
  import type { createExternalConversationController } from "$lib/chat/externalConversationController.svelte";
  import { agentRuntimeLabel, isProviderConversationRuntime, type ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";
  import { chat } from "$lib/stores/chat.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { layout } from "$lib/runtime/layout.svelte";

  let { disabled = false, agentSession, externalConversation }: {
    disabled?: boolean;
    agentSession: ReturnType<typeof createAgentSessionController>;
    externalConversation: ReturnType<typeof createExternalConversationController>;
  } = $props();
  let open = $state(false);
  let page = $state<"context" | "runtime" | "session" | "mode" | "project" | "workers">("context");
  let mode = $state("General");
  const providerConversation = $derived(isProviderConversationRuntime(agentSession.sessionRuntime));
  const runtimeLabel = $derived(agentRuntimeLabel(agentSession.sessionRuntime));
  const sessionLabel = $derived(agentSession.sessionRuntime === "muse" ? "Muse session" : agentSession.sessionRuntime === "instinct" ? "Instinct agent" : agentSession.sessionRuntime === "dots" ? "Dot" : "Grok bot");
  const active = $derived(undertakings.forChat(chat.sessionId));
  const project = $derived(active?.worktree?.split(/[\\/]/).filter(Boolean).at(-1) || active?.title);
  const worker = $derived(executionTargets.selectionFor(chat.sessionId));
  const workerLabel = $derived(executionTargets.selectionLabel(chat.sessionId));
  const workerUnavailable = $derived(executionTargets.selectionUnavailable(chat.sessionId));
  const title = $derived({context:"Chat context", runtime:"Runtime", session:sessionLabel, mode:"Mode", project:"Project", workers:"Workers"}[page]);
  $effect(() => { chat.sessionId; chat.workshopScopeId; open = false; page = "context"; });
  function home() { page = "context"; }
  function chooseRuntime(value: ChatAgentRuntime) {
    if (isProviderConversationRuntime(value)) {
      page = "session";
      void externalConversation.refresh();
    } else home();
  }
  function openSettings() {
    open = false;
    settingsNav.setActiveSection("connections");
    layout.openMore("settings");
  }
</script>

<button type="button" class="context-summary" {disabled} aria-label="Chat context" aria-haspopup="dialog" aria-expanded={open}
  onclick={() => { home(); open = true; }}>
  <span>{agentSession.sessionRuntime === "medousa" ? mode : runtimeLabel}</span>
  {#if providerConversation}
    <span class="dot">·</span><span class="project">{externalConversation.selected?.label ?? `Choose ${sessionLabel.toLowerCase()}`}</span>
  {:else}
    {#if project}<span class="dot">·</span><span class="project">{project}</span>{:else if mode === "Coder"}<span class="dot">·</span><span class="project">Choose project</span>{/if}
    {#if worker && worker.kind !== "same_as_parent"}<span class="dot">·</span><span class="worker" class:unavailable={workerUnavailable}>{workerLabel}</span>{/if}
  {/if}
  <ChevronDown size={13}/>
</button>
<MobileActionSheet bind:open {title} onback={page === "context" ? undefined : page === "session" ? () => page = "runtime" : home} full={page === "project" && !active}>
  <div hidden={page !== "context"}>
    <button type="button" class="context-row" {disabled} onclick={() => page = "runtime"}>
      <span>Runtime</span><span class="value">{runtimeLabel}</span><ChevronRight size={17}/>
    </button>
    {#if providerConversation}
      <button type="button" class="context-row" {disabled} onclick={() => { page = "session"; void externalConversation.refresh(); }}>
        <span>{sessionLabel}</span><span class="value">{externalConversation.selected?.label ?? "Choose"}</span><ChevronRight size={17}/>
      </button>
    {:else}
      {#each [{id:"mode",label:"Mode",value:mode},{id:"project",label:"Project",value:project || "Choose or create"},{id:"workers",label:"Workers",value:workerLabel}] as entry}
        <button type="button" class="context-row" {disabled} onclick={() => page = entry.id as typeof page}>
          <span>{entry.label}</span><span class="value">{entry.value}</span><ChevronRight size={17}/>
        </button>
      {/each}
      {#if workerUnavailable}<p role="status" class="unavailable">Choose an available worker destination before sending.</p>{/if}
    {/if}
  </div>
  <div hidden={page !== "runtime"}>
    <ChatRuntimePicker inline value={agentSession.sessionRuntime} {disabled} onChange={agentSession.onRuntimeChange} onchoose={chooseRuntime} onSettings={openSettings}/>
  </div>
  {#if page === "session"}
    <p class="workshop-faint mb-3 text-sm">Choose a {sessionLabel.toLowerCase()} registered on the connected workshop.</p>
    {#if externalConversation.error}<p role="alert" class="mb-3 text-sm text-content-error">{externalConversation.error}</p>{/if}
    {#if externalConversation.loading}
      <p role="status" class="flex items-center gap-2 py-4"><LoaderCircle size={18} class="animate-spin"/> Loading…</p>
    {:else if externalConversation.choices.length === 0}
      <p class="workshop-faint py-4 text-sm">No {agentSession.sessionRuntime === "muse" ? "Muse sessions" : agentSession.sessionRuntime === "instinct" ? "Instinct agents" : agentSession.sessionRuntime === "dots" ? "dots" : "Grok bots"} on this workshop. Choose the workshop hosting your connection, or set one up in External Agents.</p>
    {:else}
      <div role="group" aria-label={sessionLabel}>
        {#each externalConversation.choices as choice (choice.id)}
          <button type="button" class="context-row" aria-pressed={externalConversation.selectedId === choice.id} {disabled}
            onclick={() => { externalConversation.select(choice.id); home(); }}>
            <span class="session-name">{choice.label}</span>
            {#if externalConversation.selectedId === choice.id}<Check size={18} class="text-content-link"/>{/if}
          </button>
        {/each}
      </div>
    {/if}
    <div class="flex flex-wrap gap-4 pt-4">
      <button type="button" class="min-h-11 text-sm text-content-link" disabled={disabled || externalConversation.loading} onclick={() => void externalConversation.refresh()}>Refresh</button>
      <button type="button" class="min-h-11 text-sm text-content-link" onclick={openSettings}>Manage in External Agents</button>
    </div>
  {/if}
  <div hidden={page !== "mode"}><ChatAgentModePicker embedded sessionId={chat.sessionId} bind:label={mode} {disabled} onchoose={home}/></div>
  <div hidden={page !== "project"}><UndertakingContextChip embedded chatOnly composer visible={open && page === "project"} onrequest={() => { page = "project"; open = true; }}/></div>
  <div hidden={page !== "workers"}><ChatExecutionTargetPicker embedded sessionId={chat.sessionId} {disabled} onchoose={home}/></div>
</MobileActionSheet>

<style>
  .context-summary { display: flex; align-items: center; gap: 7px; max-width: 100%; min-height: 36px; margin: 0 8px 4px; padding: 5px 8px; font-size: 13px; color: rgb(var(--theme-text-secondary)); }
  .project, .worker { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { opacity: .4; }
  .context-row { display: flex; align-items: center; gap: 12px; width: 100%; min-height: 56px; padding: 12px 8px; text-align: left; border-bottom: 1px solid rgb(var(--theme-border) / .2); }
  .value { flex: 1; text-align: right; color: rgb(var(--theme-text-secondary)); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .session-name { flex: 1; overflow-wrap: anywhere; }
  .unavailable { color: #fbbf24; }
</style>
