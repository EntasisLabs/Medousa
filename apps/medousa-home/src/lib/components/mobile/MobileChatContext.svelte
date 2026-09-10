<script lang="ts">
  import { ChevronDown, ChevronRight } from "@lucide/svelte";
  import MobileActionSheet from "./MobileActionSheet.svelte";
  import ChatAgentModePicker from "$lib/components/chat/ChatAgentModePicker.svelte";
  import ChatExecutionTargetPicker from "$lib/components/chat/ChatExecutionTargetPicker.svelte";
  import UndertakingContextChip from "$lib/components/work/UndertakingContextChip.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";

  let { disabled = false }: { disabled?: boolean } = $props();
  let open = $state(false);
  let page = $state<"context" | "mode" | "project" | "workers">("context");
  let mode = $state("General");
  const active = $derived(undertakings.forChat(chat.sessionId));
  const project = $derived(active?.worktree?.split(/[\\/]/).filter(Boolean).at(-1) || active?.title);
  const worker = $derived(executionTargets.selectionFor(chat.sessionId));
  const workerLabel = $derived(executionTargets.selectionLabel(chat.sessionId));
  const workerUnavailable = $derived(executionTargets.selectionUnavailable(chat.sessionId));
  const title = $derived({context:"Chat context", mode:"Mode", project:"Project", workers:"Workers"}[page]);
  $effect(() => { chat.sessionId; open = false; page = "context"; });
  function home() { page = "context"; }
</script>

<button type="button" class="context-summary" {disabled} aria-haspopup="dialog" aria-expanded={open}
  onclick={() => { home(); open = true; }}>
  <span>{mode}</span>
  {#if project}<span class="dot">·</span><span class="project">{project}</span>{:else if mode === "Coder"}<span class="dot">·</span><span class="project">Choose project</span>{/if}
  {#if worker && worker.kind !== "same_as_parent"}<span class="dot">·</span><span class="worker" class:unavailable={workerUnavailable}>{workerLabel}</span>{/if}
  <ChevronDown size={13}/>
</button>
<MobileActionSheet bind:open {title} onback={page === "context" ? undefined : home} full={page === "project" && !active}>
  <div hidden={page !== "context"}>
    {#each [{id:"mode",label:"Mode",value:mode},{id:"project",label:"Project",value:project || "Choose or create"},{id:"workers",label:"Workers",value:workerLabel}] as entry}
      <button type="button" class="context-row" {disabled} onclick={() => page = entry.id as typeof page}>
        <span>{entry.label}</span><span class="value">{entry.value}</span><ChevronRight size={17}/>
      </button>
    {/each}
    {#if workerUnavailable}<p role="status" class="unavailable">Choose an available worker destination before sending.</p>{/if}
  </div>
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
  .unavailable { color: #fbbf24; }
</style>
