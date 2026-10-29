<script lang="ts">
  import { tick } from "svelte";
  import { X } from "@lucide/svelte";
  import type { ChatAgentGroup } from "$lib/utils/chatAgentGroups";
  import type { PeerProposalControls } from "./peerProposalControls";
  import { peerHandoffPresentation, peerWorkTitle } from "./peerHandoffPresentation";
  import AgentTaskDetail from "./AgentTaskDetail.svelte";

  let { group, controls, id, onOpenSubagent, onStopSubagent }: {
    group: ChatAgentGroup; controls?: PeerProposalControls; id?: string;
    onOpenSubagent?: (workId: string) => void;
    onStopSubagent?: (workId: string) => void;
  } = $props();
  let selectedId = $state<string | null>(null);
  let dialog: HTMLDialogElement;
  const selected = $derived(group.proposals.find(row => row.proposal.proposal_id === selectedId));
  $effect(() => { if (!selected) dialog?.close(); });
  async function inspect(id: string) { selectedId = id; await tick(); if (selectedId === id && selected) dialog.showModal(); }
</script>

{#if group.proposals.length || group.workers.length}
  <section {id} class="task-receipts" aria-label="Work from this turn">
    {#each group.proposals as row (row.proposal.proposal_id)}
      {@const progress = peerHandoffPresentation(row, controls?.now ?? Date.now())}
      <button type="button" class="task-receipt" class:decision={progress.needsApproval} disabled={!controls} onclick={() => void inspect(row.proposal.proposal_id)}>
        <span class="signal" aria-hidden="true">●</span>
        <span class="heading"><small>{progress.needsApproval ? 'Decision needed' : progress.status}</small><strong>{peerWorkTitle(row.proposal.request.instructions)}</strong><span>{progress.needsApproval ? 'Review the request before starting' : progress.nextActor ? `Next: ${progress.nextActor}` : 'Open task details'}</span></span>
        <span class="link">{progress.needsApproval ? 'Review request' : 'View task'} →</span>
      </button>
    {/each}
    {#if group.workers.length}
      <details class="worker-summary">
        <summary>{group.workers.length} agent task{group.workers.length === 1 ? '' : 's'} · {group.workers.some(row => row.streaming) ? 'In progress' : group.workers.every(row => row.terminal) ? 'Finished' : 'View details'}</summary>
    {#each group.workers as row (row.workId)}
      <div class="worker-receipt" data-worker-id={row.workId} tabindex="-1">
        <button type="button" disabled={!onOpenSubagent} onclick={() => onOpenSubagent?.(row.workId)}><span class="signal" aria-hidden="true">{row.streaming ? '●' : '↳'}</span><span>{row.title}</span><small>{row.attention ? 'Needs attention' : row.terminal ? 'View result →' : row.statusLine || 'Working'}</small></button>
        {#if row.streaming && onStopSubagent}<button type="button" class="stop" onclick={() => onStopSubagent?.(row.workId)} aria-label={`Stop ${row.title}`}>Stop</button>{/if}
      </div>
    {/each}
      </details>
    {/if}
  </section>
{/if}
<dialog bind:this={dialog} aria-label="Task details" onclose={() => selectedId = null} onclick={(event) => { if (event.target === dialog) dialog.close(); }}>
  <header><span>Task details</span><button type="button" aria-label="Close task details" onclick={() => dialog.close()}><X size={18} /></button></header>
  {#if selected && controls}<AgentTaskDetail row={selected} {controls} />{/if}
</dialog>

<style>
  .task-receipts { display: grid; gap: 8px; margin-block: 12px; min-width: 0; }
  button { background: transparent; color: rgb(var(--theme-text-secondary)); border: 0; cursor: pointer; text-align: left; }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 3px; }
  button:disabled { cursor: default; }
  .task-receipt { display: flex; align-items: center; gap: 12px; padding: 12px; border: 1px solid rgb(var(--theme-border)); border-radius: 9px; min-width: 0; }
  .task-receipt.decision { border-color: rgb(var(--theme-warning) / .55); }
  .signal { color: rgb(var(--theme-link)); flex-shrink: 0; font-size: 11px; }
  .decision .signal, .decision .link, .decision small { color: rgb(var(--theme-warning)); }
  .heading { flex: 1; min-width: 0; }
  .heading > * { display: block; overflow-wrap: anywhere; }
  strong { font-size: 13px; color: rgb(var(--theme-text-primary)); font-weight: 500; }
  small, .heading > span, .link { font-size: 12px; }
  .heading > span { margin-top: 3px; }
  .worker-summary { min-width: 0; border-left: 2px solid rgb(var(--theme-border)); padding-left: 10px; }
  summary { color: rgb(var(--theme-text-secondary)); font-size: 12px; cursor: pointer; padding: 6px 0; }
  .worker-receipt { display: flex; gap: 8px; align-items: center; }
  .worker-receipt > button:first-child > span:nth-child(2) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: min(48ch, 55vw); }
  .worker-receipt > button:first-child { min-width: 0; }
  .worker-receipt > button:first-child { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; padding: 5px 0; font-size: 12px; }
  .worker-receipt small { color: rgb(var(--theme-text-tertiary)); }
  .stop { margin-left: auto; font-size: 12px; }
  dialog { position: fixed; inset: 0 0 0 auto; margin: 0; width: min(480px, 100%); max-width: 100%; height: 100%; max-height: 100%; overflow-y: auto; padding: 20px; border: 1px solid rgb(var(--theme-border)); background: rgb(var(--theme-card)); color: rgb(var(--theme-text-primary)); }
  dialog::backdrop { background: rgb(0 0 0 / .25); }
  header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 24px; font-size: 12px; }
  @media (max-width: 500px) { .task-receipt { flex-wrap: wrap; } .link { margin-left: 23px; } }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
