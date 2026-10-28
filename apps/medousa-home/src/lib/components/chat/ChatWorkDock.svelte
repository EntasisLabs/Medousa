<script lang="ts">
  import { GitBranch, X } from "@lucide/svelte";
  import { tick, type Snippet } from "svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import type { SubagentRow } from "$lib/utils/subagentRows";
  import type { PeerProposalControls } from "./peerProposalControls";
  import PeerProposalRow from "./PeerProposalRow.svelte";
  import { peerHandoffPresentation, peerWorkTitle } from "./peerHandoffPresentation";

  let { sessionId, proposals = [], workers = [], controls, onOpenWorker, children }: {
    sessionId: string;
    proposals?: PeerProposalReviewRecord[];
    workers?: SubagentRow[];
    controls?: PeerProposalControls;
    onOpenWorker: (workId: string) => void;
    children?: Snippet;
  } = $props();
  let expanded = $state(false);
  let selectedId = $state<string | null>(null);
  let dialog: HTMLDialogElement;
  const selected = $derived(proposals.find(row => row.proposal.proposal_id === selectedId));
  const count = $derived(proposals.length + workers.length);
  const orderedProposals = $derived([...proposals].sort((a, b) => priority(a) - priority(b)));
  function priority(row: PeerProposalReviewRecord) {
    const progress = peerHandoffPresentation(row, controls?.now ?? Date.now());
    return progress.needsApproval ? 0 : progress.attention ? 1 : row.handoff?.state === 'awaiting_sender_review' ? 2 : row.receipt ? 4 : 3;
  }
  const needsYou = $derived(proposals.filter(row => peerHandoffPresentation(row, controls?.now ?? Date.now()).needsApproval).length);
  const working = $derived(workers.filter(row => row.streaming).length + proposals.filter(row => peerHandoffPresentation(row, controls?.now ?? Date.now()).status === 'Agent working').length);
  const review = $derived(proposals.filter(row => peerHandoffPresentation(row, controls?.now ?? Date.now()).status === 'Awaiting sender review').length);
  $effect(() => { sessionId; expanded = false; selectedId = null; dialog?.close(); });
  $effect(() => { if (!selected) dialog?.close(); });
  function projectLabel(row: PeerProposalReviewRecord) {
    const target = row.proposal.request.target;
    const executionWorkshop = workshops.workshops.find(workshop => workshop.pairing?.workshopDeviceId === target.execution_runtime_id);
    const local = target.authority_id === connection.health?.runtime?.authority_id || executionWorkshop?.id === workshops.activeWorkshopId;
    const id = row.proposal.request.forge_work_id;
    const title = local ? (undertakings.active?.workId === id ? undertakings.active.title : undertakings.items.find(item => item.id === id)?.title) : null;
    return `${executionWorkshop?.label ?? (local ? workshops.activeLabel : 'Execution workshop')} / ${title ?? `Project ${id.slice(-8)}`}`;
  }
  async function inspect(id: string) { selectedId = id; await tick(); if (selectedId === id && selected) dialog.showModal(); }
</script>

<div class="work-dock">
  {#if count}
    <button type="button" class="work-pill" aria-expanded={expanded} onclick={() => expanded = !expanded}>
      <GitBranch size={13} aria-hidden="true" /> {count} agent{count === 1 ? '' : 's'}
      {#if needsYou}<span class="attention">· {needsYou} needs you</span>{/if}
    </button>
  {/if}
  {#if children}{@render children()}{/if}
</div>
{#if expanded && count}
  <section class="work-tray" aria-label="Work in this chat">
    <header><div><strong>Work & attention</strong><p>This chat · {working} working · {review} review pending</p></div><button type="button" aria-label="Collapse work list" onclick={() => expanded = false}><X size={16} /></button></header>
    <p class="hint">Opening a task never approves or starts it.</p>
    <div class="tasks">
      {#each orderedProposals as row (row.proposal.proposal_id)}
        {@const progress = peerHandoffPresentation(row, controls?.now ?? Date.now())}
        <button type="button" class="task" onclick={() => inspect(row.proposal.proposal_id)}>
          <strong>{peerWorkTitle(row.proposal.request.instructions)}</strong>
          <small>{projectLabel(row)} · Request {row.proposal.proposal_id.slice(-8)}</small>
          <span>{controls?.actionErrors?.[row.proposal.proposal_id] && row.decision?.approved && !row.binding && !row.receipt ? 'Start not confirmed' : progress.status}{progress.nextActor ? ` · Next: ${progress.nextActor}` : ''}</span>
        </button>
      {/each}
      {#each workers as row (row.workId)}
        <button type="button" class="task" onclick={() => { expanded = false; onOpenWorker(row.workId); }}>
          <strong>{row.title}</strong><small>Agent {row.workId.slice(-8)}</small>
          <span>{row.attention ? 'Needs attention' : row.terminal ? 'Session ended · View result' : row.statusLine}</span>
        </button>
      {/each}
    </div>
    {#if controls?.feedback}<p role="status" class="hint">{controls.feedback}</p>{/if}
    {#if controls?.hasMore}<button type="button" disabled={controls.busy || !controls.available} onclick={() => void controls?.loadMore()}>Load more requests</button>{/if}
  </section>
{/if}
<dialog bind:this={dialog} class="task-thread" aria-label="Agent task details" onclose={() => selectedId = null} onclick={(event) => { if (event.target === dialog) dialog.close(); }}>
  {#if selected && controls}
    <header><div><strong>Agent task</strong><p>This chat · Assignment, result & review</p></div><button type="button" aria-label="Close agent task" onclick={() => dialog.close()}><X size={18} /></button></header>
    <p class="hint">A returned result is not proof of changes, acceptance, merging or publication.</p>
    <PeerProposalRow row={selected} {controls} expanded />
    {#if controls.feedback}<p role="status" class="hint">{controls.feedback}</p>{/if}
  {/if}
</dialog>

<style>
  .work-dock { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0 16px 6px; }
  button { color: rgb(var(--theme-text-secondary)); background: transparent; border: 0; cursor: pointer; }
  .work-pill { display: inline-flex; align-items: center; gap: 6px; border: 1px solid rgb(var(--theme-border)); border-radius: 999px; padding: 5px 10px; font-size: 12px; }
  button:hover { background: rgb(var(--theme-card-hover)); }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 2px; }
  .attention { color: rgb(var(--theme-warning)); }
  .work-tray { margin: 0 16px 8px; padding: 12px; border: 1px solid rgb(var(--theme-border)); border-radius: 12px; background: rgb(var(--theme-card)); max-height: min(45vh, 400px); overflow: auto; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  strong { color: rgb(var(--theme-text-primary)); font-size: 13px; }
  p, small, .hint { color: rgb(var(--theme-text-tertiary)); font-size: 12px; }
  .hint { margin: 10px 0; }
  .tasks { display: grid; }
  .task { text-align: left; padding: 10px 4px; border-top: 1px solid rgb(var(--theme-border) / .5); min-width: 0; }
  .task strong, .task small, .task span { display: block; overflow-wrap: anywhere; }
  .task span { font-size: 12px; margin-top: 4px; }
  .task-thread { position: fixed; inset: 0 0 0 auto; margin: 0; width: min(520px, 100%); height: 100%; max-height: 100%; max-width: 100%; padding: 20px; border: 1px solid rgb(var(--theme-border)); background: rgb(var(--theme-card)); color: rgb(var(--theme-text-primary)); overflow-y: auto; }
  .task-thread::backdrop { background: rgb(0 0 0 / .25); }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
