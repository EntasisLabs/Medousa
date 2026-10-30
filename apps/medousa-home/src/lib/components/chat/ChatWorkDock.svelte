<script lang="ts">
  import { Bot, ChevronDown, CircleAlert, X } from "@lucide/svelte";
  import { tick, type Snippet } from "svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import type { SubagentRow } from "$lib/utils/subagentRows";
  import type { PeerProposalControls } from "./peerProposalControls";
  import AgentTaskDetail from "./AgentTaskDetail.svelte";
  import { readWorkDismissals, workHistoryKey, writeWorkDismissals } from "./workDockHistory";
  import { peerHandoffPresentation, peerWorkTitle } from "./peerHandoffPresentation";

  let { sessionId, proposals = [], workers = [], controls, onOpenWorker, children, projectNeedsReview = false, onOpenProjectReview }: {
    sessionId: string;
    proposals?: PeerProposalReviewRecord[];
    workers?: SubagentRow[];
    controls?: PeerProposalControls;
    onOpenWorker: (workId: string) => void;
    children?: Snippet;
    projectNeedsReview?: boolean;
    onOpenProjectReview?: () => void;
  } = $props();
  let expanded = $state(false);
  let selectedId = $state<string | null>(null);
  let dialog: HTMLDialogElement;
  let tray = $state<HTMLDialogElement>();
  let trigger = $state<HTMLButtonElement>();
  let dock: HTMLDivElement;
  let position = $state({ left: 12, top: 12, maxHeight: 440 });
  function place() {
    if (!dock || !tray) return;
    const rect = dock.getBoundingClientRect();
    const width = Math.min(440, window.innerWidth - 24);
    const above = rect.top - 22;
    const below = window.innerHeight - rect.bottom - 22;
    const useAbove = above >= 260 || above > below;
    const maxHeight = Math.max(100, Math.min(440, useAbove ? above : below));
    position = { left: Math.max(12, Math.min(rect.left, window.innerWidth - width - 12)), top: useAbove ? Math.max(12, rect.top - 10 - Math.min(tray.offsetHeight || maxHeight, maxHeight)) : rect.bottom + 10, maxHeight };
  }
  function dismiss(restore = true) { expanded = false; tray?.close(); if (restore) trigger?.focus(); }
  function outside(event: PointerEvent) { if (expanded && event.target instanceof Node && !tray?.contains(event.target) && !dock?.contains(event.target)) dismiss(false); }
  $effect(() => {
    if (!expanded || !tray) return;
    tray.show(); place(); tray.focus();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(place); observer.observe(tray);
    return () => observer.disconnect();
  });
  const selected = $derived(proposals.find(row => row.proposal.proposal_id === selectedId));
  let dismissedIds = $state<string[]>([]);
  let showDismissed = $state(false);
  const historyKey = $derived(workHistoryKey(workshops.activeWorkshopId ?? '', connection.health?.runtime?.authority_id ?? '', sessionId));
  let loadedHistoryKey = $state('');
  $effect(() => { dismissedIds = readWorkDismissals(historyKey); loadedHistoryKey = historyKey; showDismissed = false; });
  $effect(() => {
    if (loadedHistoryKey !== historyKey) return;
    const active = new Set(entries.filter(row => row.group !== 'Finished').map(row => row.id));
    const retained = dismissedIds.filter(id => !active.has(id));
    if (retained.length !== dismissedIds.length) {
      dismissedIds = retained;
      writeWorkDismissals(historyKey, retained);
    }
  });
  type TaskEntry = { id: string; title: string; status: string; project?: string; group: 'Needs attention' | 'In progress' | 'Finished'; proposalId?: string; workId?: string };
  const entries = $derived.by(() => {
    const rows: TaskEntry[] = proposals.map(row => {
      const progress = peerHandoffPresentation(row, controls?.now ?? Date.now());
      const attention = progress.needsApproval || progress.attention || row.handoff?.state === 'awaiting_sender_review';
      const finished = !attention && Boolean(row.receipt || row.handoff?.state === 'accepted');
      return { id: `proposal:${row.proposal.proposal_id}`, proposalId: row.proposal.proposal_id,
        title: peerWorkTitle(row.proposal.request.instructions), project: projectLabel(row),
        status: controls?.actionErrors?.[row.proposal.proposal_id] && row.decision?.approved && !row.binding && !row.receipt ? 'Start not confirmed' : `${progress.status}${progress.nextActor ? ` · Next: ${progress.nextActor}` : ''}`,
        group: attention ? 'Needs attention' : finished ? 'Finished' : 'In progress' };
    });
    for (const row of workers) rows.push({ id: `worker:${row.workId}`, workId: row.workId, title: peerWorkTitle(row.title),
      status: row.attention ? 'Needs attention' : row.terminal ? 'Session ended · View result' : row.statusLine,
      group: row.attention ? 'Needs attention' : row.terminal ? 'Finished' : 'In progress' });
    return rows;
  });
  const activeCount = $derived(entries.filter(row => row.group !== 'Finished').length);
  const finishedCount = $derived(entries.filter(row => row.group === 'Finished').length);
  const dismissedCount = $derived(entries.filter(row => row.group === 'Finished' && dismissedIds.includes(row.id)).length);
  const count = $derived(entries.length);
  function setDismissed(id: string, hide: boolean) {
    dismissedIds = hide ? [...new Set([...dismissedIds, id])] : dismissedIds.filter(value => value !== id);
    writeWorkDismissals(historyKey, dismissedIds);
  }
  function openEntry(row: TaskEntry) {
    if (row.proposalId) { void inspect(row.proposalId); return; }
    if (row.workId) { dismiss(); onOpenWorker(row.workId); }
  }
  const needsYou = $derived(proposals.filter(row => peerHandoffPresentation(row, controls?.now ?? Date.now()).needsApproval).length + (projectNeedsReview ? 1 : 0));
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
  async function inspect(id: string) { expanded = false; tray?.close(); trigger?.focus(); if (controls?.inspect) { await tick(); controls.inspect(id); return; } selectedId = id; await tick(); if (selectedId === id && selected) dialog.showModal(); }
</script>

<svelte:window onpointerdown={outside} onresize={place} onkeydown={(event) => { if (expanded && event.key === 'Escape') { event.preventDefault(); dismiss(); } }} />

<div class="work-dock" bind:this={dock}>
  {#if count}
    <button bind:this={trigger} type="button" class="work-pill" aria-label={working ? `${working} task${working === 1 ? '' : 's'} working` : activeCount ? `${activeCount} task${activeCount === 1 ? '' : 's'}` : 'Task history'} aria-haspopup="dialog" aria-expanded={expanded} onclick={() => expanded = !expanded}>
      <Bot size={16} strokeWidth={1.75} aria-hidden="true" />
      <span class="work-label">{#if activeCount}{working || activeCount} task{(working || activeCount) === 1 ? '' : 's'}{:else}Task history{/if}{#if working}<span class="working-label">{' working'}</span>{/if}</span>
      <ChevronDown size={12} aria-hidden="true" class={expanded ? 'chevron expanded' : 'chevron'} />
    </button>

  {/if}
  {#if needsYou}<button type="button" class="decision-link" aria-expanded={expanded} onclick={() => { if (projectNeedsReview && !count) onOpenProjectReview?.(); else expanded = !expanded; }}><CircleAlert size={14} aria-hidden="true" /><span class="decision-label">{needsYou} need{needsYou === 1 ? 's' : ''} your decision</span></button>{/if}
  {#if children}{@render children()}{/if}
</div>
{#if expanded && count}
  <dialog bind:this={tray} class="work-tray" aria-label="Work in this chat" tabindex="-1" onclose={() => expanded = false}
    style:left={`${position.left}px`} style:top={`${position.top}px`} style:max-height={`${position.maxHeight}px`}>
    <header><div><strong>Work & attention</strong><p>This chat · {working} working · {review} review pending</p></div><button type="button" aria-label="Collapse work list" onclick={() => dismiss()}><X size={16} /></button></header>
    <p class="hint">Current work first. Finished sessions stay in history; decisions remain explicit.</p>
    <div class="tasks">
      {#if projectNeedsReview}<button type="button" class="task" onclick={() => { expanded = false; onOpenProjectReview?.(); }}><strong>Review project changes</strong><span>Your decision · Not applied</span></button>{/if}
      {#each ['Needs attention', 'In progress'] as group}
        {#if entries.some(row => row.group === group)}
          <h3>{group} · {entries.filter(row => row.group === group).length}</h3>
          {#each entries.filter(row => row.group === group) as row (row.id)}
            <button type="button" class="task" onclick={() => openEntry(row)}>
              <strong>{row.title}</strong>{#if row.project}<small>{row.project}</small>{/if}<span>{row.status}</span>
            </button>
          {/each}
        {/if}
      {/each}
      {#if !activeCount && !projectNeedsReview}<p class="hint">No active work or pending decisions in this chat.</p>{/if}
      {#if finishedCount}
        <details class="finished-history">
          <summary>Finished · {finishedCount - dismissedCount}{#if dismissedCount} · {dismissedCount} dismissed{/if}</summary>
          <p class="hint">Session ended does not mean accepted or applied. Dismiss only hides an item here.</p>
          {#if dismissedCount}<button type="button" class="history-toggle" onclick={() => showDismissed = !showDismissed}>{showDismissed ? 'Hide dismissed' : 'Show dismissed'}</button>{/if}
          {#each entries.filter(row => row.group === 'Finished' && (showDismissed || !dismissedIds.includes(row.id))) as row (row.id)}
            <div class="finished-row">
              <button type="button" class="task" onclick={() => openEntry(row)}><strong>{row.title}</strong>{#if row.project}<small>{row.project}</small>{/if}<span>{row.status}</span></button>
              <button type="button" class="history-action" aria-label={`${dismissedIds.includes(row.id) ? 'Restore' : 'Dismiss'} ${row.title}`} onclick={() => setDismissed(row.id, !dismissedIds.includes(row.id))}>{dismissedIds.includes(row.id) ? 'Restore' : 'Dismiss'}</button>
            </div>
          {/each}
        </details>
      {/if}
    </div>
    {#if controls?.feedback}<p role="status" class="hint">{controls.feedback}</p>{/if}
    {#if controls?.hasMore}<button type="button" disabled={controls.busy || !controls.available} onclick={() => void controls?.loadMore()}>Load more requests</button>{/if}
  </dialog>
{/if}
<dialog bind:this={dialog} class="task-thread" aria-label="Agent task details" onclose={() => selectedId = null} onclick={(event) => { if (event.target === dialog) dialog.close(); }}>
  {#if selected && controls}
    <header><div><strong>Agent task</strong><p>This chat · Assignment, result & review</p></div><button type="button" aria-label="Close agent task" onclick={() => dialog.close()}><X size={18} /></button></header>
    <p class="hint">A returned result is not proof of changes, acceptance, merging or publication.</p>
    <AgentTaskDetail row={selected} {controls} />
    {#if controls.feedback}<p role="status" class="hint">{controls.feedback}</p>{/if}
  {/if}
</dialog>

<style>
  .work-dock { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .work-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .work-pill :global(.chevron) { flex-shrink: 0; transition: transform 120ms ease; }
  .work-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .work-pill :global(svg) { flex-shrink: 0; }
  .work-pill :global(.chevron.expanded) { transform: rotate(180deg); }
  button { color: rgb(var(--theme-text-secondary)); background: transparent; border: 0; cursor: pointer; }
  .work-pill { display: inline-flex; min-width: 0; min-height: 32px; align-items: center; gap: 7px; padding: 6px 8px; margin-left: -8px; border-radius: 9px; font-size: 12px; line-height: 20px; transition: background-color 120ms ease, color 120ms ease; }
  .work-pill[aria-expanded='true'] { background: rgb(var(--theme-card-hover)); color: rgb(var(--theme-text-primary)); }
  .decision-link { display: inline-flex; min-height: 32px; flex-shrink: 0; align-items: center; gap: 5px; color: rgb(var(--theme-warning)); font-size: 11px; padding: 6px; border-radius: 9px; }
  @media (max-width: 600px) { .work-pill :global(.chevron), .working-label { display: none; } .decision-label { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip-path: inset(50%); white-space: nowrap; } }
  button:hover { background: rgb(var(--theme-card-hover)); }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 2px; }
  .work-tray { position: fixed; z-index: 45; box-sizing: border-box; inset: auto; margin: 0; width: min(440px, calc(100vw - 24px)); padding: 16px; border: 1px solid rgb(var(--theme-border)); border-radius: 20px; background: rgb(var(--theme-card)); color: rgb(var(--theme-text-primary)); overflow: auto; outline: none; overscroll-behavior: contain; box-shadow: 0 12px 40px rgb(0 0 0 / .25); }
  .work-tray::backdrop { background: transparent; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  strong { color: rgb(var(--theme-text-primary)); font-size: 13px; }
  p, small, .hint { color: rgb(var(--theme-text-tertiary)); font-size: 12px; }
  .hint { margin: 10px 0; }
  .tasks { display: grid; }
  h3 { margin: 12px 0 4px; font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  .finished-history { margin-top: 12px; border-top: 1px solid rgb(var(--theme-border) / .5); padding-top: 12px; }
  summary { cursor: pointer; font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  summary:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 2px; }
  .finished-row { display: flex; align-items: center; gap: 8px; }
  .finished-row .task { flex: 1; }
  .history-action, .history-toggle { padding: 6px; border-radius: 6px; font-size: 11px; }
  .task { text-align: left; padding: 10px 4px; border-top: 1px solid rgb(var(--theme-border) / .5); min-width: 0; }
  .task strong, .task small, .task span { display: block; overflow-wrap: anywhere; }
  .task span { font-size: 12px; margin-top: 4px; }
  .task-thread { position: fixed; inset: 0 0 0 auto; margin: 0; width: min(520px, 100%); height: 100%; max-height: 100%; max-width: 100%; padding: 20px; border: 1px solid rgb(var(--theme-border)); background: rgb(var(--theme-card)); color: rgb(var(--theme-text-primary)); overflow-y: auto; }
  .task-thread::backdrop { background: rgb(0 0 0 / .25); }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
