<script lang="ts">
  import { ArrowUpRight, Bot, Folder, MessageSquareText } from "@lucide/svelte";
  import AgentWorkContextLine from "./AgentWorkContextLine.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import type { PeerProposalControls } from "./peerProposalControls";
  import { peerRuntimeLabel } from "./peerProgress";
  import { peerHandoffPresentation, peerWorkTitle } from "./peerHandoffPresentation";

  let { row: current, controls }: { row: PeerProposalReviewRecord; controls: PeerProposalControls } = $props();
  const completed = $derived(current.receipt ?? null);
  const progress = $derived(peerHandoffPresentation(current, controls.now));
  const executionWorkshop = $derived(workshops.workshops.find(workshop => workshop.pairing?.workshopDeviceId === current.proposal.request.target.execution_runtime_id));
  const onExecutionWorkshop = $derived(current.proposal.request.target.authority_id === connection.health?.runtime?.authority_id || executionWorkshop?.id === workshops.activeWorkshopId);
  const projectTitle = $derived(onExecutionWorkshop ? (undertakings.active?.workId === current.proposal.request.forge_work_id ? undertakings.active.title : undertakings.items.find(item => item.id === current.proposal.request.forge_work_id)?.title) : null);
  const workTitle = $derived(peerWorkTitle(current.proposal.request.instructions, projectTitle));
  const senderLabel = $derived(chat.sessions.find(session => session.session_id === current.proposal.request.owner_session.session_id)?.display_name || "Sender");
  const agentLabel = $derived(peerRuntimeLabel(current.proposal.request.target.runtime));
  const workshopLabel = $derived(executionWorkshop?.label ?? (onExecutionWorkshop ? workshops.activeLabel : "Execution workshop"));
  const adopting = $derived(Boolean(current.proposal.request.existing_agent_session_id));
  const expired = $derived(Date.parse(current.proposal.expires_at) <= controls.now);
  const busy = $derived(controls.busyId === current.proposal.proposal_id);
  const available = $derived(controls.available);
  const activity = $derived(progress.activity ?? (completed ? completed.result.split(/\r?\n/).find(line => line.trim())?.trim() : null));
  const state = $derived(progress.attention || progress.needsApproval ? "attention" : current.handoff?.state === "awaiting_sender_review" ? "review" : completed ? "complete" : "working");
</script>

<article class="proposal-row">
  <AgentWorkContextLine title={workTitle} status={progress.status} activity={activity} {state} attention={progress.attention}>
    <div class="participants">
      <div class="participant">
        <Bot size={16} aria-hidden="true" />
        <div class="participant-info"><p>{agentLabel}</p><small>{progress.workerStatus} · {workshopLabel}{adopting ? ' · existing work' : ''}</small></div>
        {#if current.binding && onExecutionWorkshop && current.proposal.request.target.runtime === 'medousa'}
          <button type="button" class="text-action" onclick={() => void controls.openExecution(current, 'chat')}>Open chat <ArrowUpRight size={13} aria-hidden="true" /></button>
        {/if}
      </div>
      <div class="participant">
        <MessageSquareText size={16} aria-hidden="true" />
        <div class="participant-info"><p>{senderLabel}</p><small>{progress.senderResponsible ? 'Owns the request' : current.handoff ? 'Ownership passed to agent' : 'Requested this work'} · {progress.senderStatus}</small></div>
      </div>
    </div>
    <div class="detail-actions">
      {#if onExecutionWorkshop}
        <button type="button" class="detail-action" onclick={() => void controls.openExecution(current, 'project')}><Folder size={13} aria-hidden="true" /> Open project</button>
      {:else if executionWorkshop}
        <button type="button" class="detail-action" onclick={() => void controls.openExecution(current, 'workshop')}><ArrowUpRight size={13} aria-hidden="true" /> Open {workshopLabel}</button>
      {/if}
    </div>
    {#if completed}
      <details class="secondary-details"><summary>Agent result</summary><p class="long-text">{completed.result}</p></details>
    {/if}
    {#if current.handoff?.review}
      <details class="secondary-details"><summary>Review</summary><p class="long-text">{current.handoff.review.reason}</p></details>
    {/if}
    {#if progress.lastActivity && progress.lastActivity !== current.progress?.current_activity}
      <p class="last-activity">Last action: {progress.lastActivity}{progress.lastActivityStatus ? ` · ${progress.lastActivityStatus}` : ''}</p>
    {/if}
    {#if progress.lastUpdate}<p class="last-activity">Updated {progress.lastUpdate}</p>{/if}
    {#if progress.notice}<p class="notice">{progress.notice}</p>{/if}
    <details class="secondary-details"><summary>Assignment</summary><p class="long-text">{current.proposal.request.instructions}</p></details>
    <details class="secondary-details">
      <summary>Details</summary>
      <dl>
        <dt>Work item</dt><dd>{current.proposal.request.forge_work_id}</dd>
        <dt>Shared conversation ranges</dt>
        {#each current.proposal.request.context.sources as source}
          <dd>{source.selection.session.session_id} · entries {(source.selection.after_entry_seq ?? 0) + 1}–{source.selection.through_entry_seq} · {source.selection_digest}</dd>
        {/each}
        {#if current.handoff}
          <dt>Responsibility</dt><dd>{(current.handoff.policy.responsibility ?? 'retain') === 'retain' ? 'Sender retains responsibility' : 'Agent takes responsibility after acceptance'}</dd>
          <dt>Completion</dt><dd>{(current.handoff.policy.completion ?? 'sender_review') === 'sender_review' ? 'Sender reviews the returned result' : 'Worker result completes the request'}</dd>
          <dt>Sender updates</dt><dd>{current.handoff.policy.wake_on_accepted !== false ? 'On acceptance' : 'No acceptance update'} · {current.handoff.policy.wake_on_terminal !== false ? 'On completion' : 'No completion update'}</dd>
          <dt>Contact</dt><dd>{current.handoff.policy.contact?.kind === 'silent' ? 'No user follow-up' : current.handoff.policy.contact?.kind === 'participant' ? 'Selected participant' : current.handoff.policy.contact?.kind === 'channel' ? 'Selected channel' : 'Return to the originating conversation'}</dd>
        {:else}
          <dt>Owner continuation</dt><dd>{current.proposal.continue_owner ? 'Report the verified result in the originating conversation' : 'No owner continuation requested'}</dd>
        {/if}
        <dt>Execution workshop</dt><dd>{current.proposal.request.target.execution_runtime_id} · {current.proposal.request.target.authority_id}</dd>
        <dt>Channel</dt><dd>{current.proposal.request.channel.channel_id}</dd>
        {#if current.binding}<dt>Agent session</dt><dd>{current.binding.agent_session_id}</dd>{/if}
        <dt>Expires</dt><dd>{new Date(current.proposal.expires_at).toLocaleString()}{expired ? ' · expired' : ''}</dd>
        <dt>Request</dt><dd>{current.proposal.proposal_id}</dd>
      </dl>
    </details>
  </AgentWorkContextLine>
  {#if !completed && !current.binding && progress.needsApproval}
    <div class="approval-actions">
      {#if current.decision?.approved}
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={controls.busy || expired || !available} onclick={() => void controls.action(current, 'dispatch')}>{busy ? 'Starting…' : 'Start approved work'}</button>
      {:else}
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={controls.busy || expired || !available} onclick={() => void controls.action(current, 'approve_and_dispatch')}>{busy ? 'Starting…' : adopting ? 'Approve & adopt' : 'Approve & start'}</button>
        <button type="button" class="btn btn-sm variant-ghost-surface" disabled={controls.busy || !available} onclick={() => void controls.action(current, 'deny')}>Decline</button>
      {/if}
      {#if expired}<span class="notice">Expired</span>{/if}
    </div>
  {/if}
</article>

<style>
  .proposal-row { min-width: 0; border-top: 1px solid rgb(var(--theme-border) / .6); }
  .participants { display: flex; flex-direction: column; gap: 12px; }
  .participant { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .participant > :global(svg) { flex-shrink: 0; color: rgb(var(--theme-text-tertiary)); }
  .participant-info { flex: 1; min-width: 0; }
  .participant-info p { font-size: 13px; color: rgb(var(--theme-text-primary)); }
  .participant-info small { display: block; font-size: 12px; color: rgb(var(--theme-text-secondary)); overflow-wrap: anywhere; }
  .text-action { display: inline-flex; align-items: center; gap: 5px; flex-shrink: 0; border: 0; background: transparent; padding: 4px 0; font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  .text-action:hover { color: rgb(var(--theme-text-primary)); }
  .detail-actions, .approval-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-top: 12px; }
  .detail-action { display: inline-flex; align-items: center; gap: 6px; border: 1px solid rgb(var(--theme-border)); border-radius: 7px; padding: 6px 10px; background: transparent; color: rgb(var(--theme-text-secondary)); font-size: 12px; }
  .detail-action:hover { background: rgb(var(--theme-card-hover)); }
  .secondary-details { margin-top: 12px; color: rgb(var(--theme-text-secondary)); font-size: 12px; }
  .secondary-details summary { cursor: pointer; }
  .long-text { white-space: pre-wrap; overflow-wrap: anywhere; margin-top: 8px; font-size: 13px; }
  dl { margin-top: 8px; overflow-wrap: anywhere; }
  dt { color: rgb(var(--theme-text-tertiary)); margin-top: 8px; }
  .last-activity, .notice { margin-top: 8px; color: rgb(var(--theme-text-secondary)); font-size: 12px; overflow-wrap: anywhere; }
  .approval-actions { margin: 10px 14px 14px 40px; }
  @media (max-width: 480px) { .participant { flex-wrap: wrap; } .participant-info { flex-basis: calc(100% - 26px); } .participant .text-action { margin-left: 26px; } }
  @media (pointer: coarse) { .text-action, .detail-action, .secondary-details summary { min-height: 44px; } }
</style>
