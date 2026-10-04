<script lang="ts">
  import { untrack } from "svelte";
  import { ArrowUpRight, Bot, Folder, MessageSquareText } from "@lucide/svelte";
  import AgentWorkContextLine from "./AgentWorkContextLine.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { actOnPeerProposal, listPeerProposals, proposalExecutionTransport } from "$lib/daemon/coordination";
  import type { PeerProposalInboxResponse, PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { isTauri } from "$lib/platform";
  import { requestRemotePeerCompletionSync } from "$lib/remotePeerCompletionSync";
  import { peerRuntimeLabel } from "./peerProgress";
  import { peerHandoffPresentation, peerWorkTitle } from "./peerHandoffPresentation";

  let { sessionId, mobile = false }: { sessionId: string | null; mobile?: boolean } = $props();
  let rows = $state<PeerProposalReviewRecord[]>([]);
  let cursors = $state<{ runtime: string | null; cursor: string }[]>([]);
  let busy = $state(false);
  let feedback = $state<string | null>(null);
  let now = $state(Date.now());
  let epoch = 0;
  let revision = 0;
  let pageAfter = new Map<string | null, string>();
  let selectedRuntime: string | null = null;
  let selectedId: string | undefined;
  let proposalOrigins = new Map<string, string | null>();
  const current = $derived(rows[0] ?? null);
  const completed = $derived(current?.receipt ?? null);
  const progress = $derived(current ? peerHandoffPresentation(current, now) : null);
  const executionWorkshop = $derived(current ? proposalWorkshop(current.proposal.request.target.execution_runtime_id) : null);
  const onExecutionWorkshop = $derived(Boolean(current && (
    current.proposal.request.target.authority_id === connection.health?.runtime?.authority_id
    || executionWorkshop?.id === workshops.activeWorkshopId
  )));
  const projectTitle = $derived(current && onExecutionWorkshop
    ? (undertakings.active?.workId === current.proposal.request.forge_work_id ? undertakings.active.title
      : undertakings.items.find(item => item.id === current.proposal.request.forge_work_id)?.title)
    : null);
  const workTitle = $derived(current ? peerWorkTitle(current.proposal.request.instructions, projectTitle) : "Delegated work");
  const senderLabel = $derived(chat.sessions.find(session => session.session_id === sessionId)?.display_name || "Sender");
  const agentLabel = $derived(current ? peerRuntimeLabel(current.proposal.request.target.runtime) : "Agent");
  const workshopLabel = $derived(executionWorkshop?.label ?? (onExecutionWorkshop ? workshops.activeLabel : "Execution workshop"));
  const adopting = $derived(Boolean(current?.proposal.request.existing_agent_session_id));
  const profileScope = $derived(connection.health?.active_profile_id ?? "");
  const proposalRuntimes = $derived([
    null,
    ...workshops.workshops
      .filter(workshop => workshop.kind === "portal" || workshop.kind === "paired")
      .map(workshop => workshop.pairing?.workshopDeviceId?.trim() || "")
      .filter(Boolean),
  ]);
  const available = $derived(isTauri() && connection.online && !workshops.switching && (
    Boolean(connection.health?.runtime?.advertised_capabilities.includes("coordination.operator_proposals.v1"))
    || proposalRuntimes.length > 1
  ));
  const expired = $derived(current ? Date.parse(current.proposal.expires_at) <= now : false);
  function proposalWorkshop(runtimeId: string) {
    return workshops.workshops.find(workshop => workshop.pairing?.workshopDeviceId === runtimeId);
  }

  $effect(() => {
    const session = sessionId;
    const workshop = workshops.activeWorkshopId;
    const profile = profileScope;
    const enabled = available;
    const token = ++epoch;
    rows = []; cursors = []; feedback = null; busy = false;
    pageAfter = new Map(); selectedId = undefined; selectedRuntime = null; proposalOrigins = new Map();
    if (!session || !enabled) return;
    let loading = false;
    const refresh = async () => {
      if (loading || untrack(() => busy) || document.visibilityState === "hidden") return;
      void requestRemotePeerCompletionSync();
      loading = true;
      const requestRevision = revision;
      try {
        const settled = await Promise.allSettled(
          proposalRuntimes.map(async runtime => ({ runtime, response: await listPeerProposals(
            session, runtime, pageAfter.get(runtime), runtime === selectedRuntime ? selectedId : undefined,
          ) })),
        );
        const responses = settled
          .filter((result): result is PromiseFulfilledResult<{ runtime: string | null; response: PeerProposalInboxResponse }> => result.status === "fulfilled")
          .map(result => result.value);
        if (!responses.length) throw settled.find(result => result.status === "rejected")?.reason ?? new Error("Proposal inbox unavailable");
        if (token === epoch && requestRevision === revision) {
          applyResponses(responses);
          const selectedIndex = proposalRuntimes.indexOf(selectedRuntime);
          feedback = selectedId && settled[selectedIndex]?.status === "rejected"
            ? "Progress could not be refreshed on the execution workshop. Showing the last known activity."
            : null;
        }
      } catch (error) {
        if (token === epoch && requestRevision === revision && untrack(() => rows.length > 0)) feedback = String(error);
      } finally { loading = false; }
    };
    void workshop; void profile;
    void refresh();
    const timer = setInterval(() => { now = Date.now(); void refresh(); }, 15_000);
    document.addEventListener("visibilitychange", refresh);
    return () => { ++epoch; clearInterval(timer); document.removeEventListener("visibilitychange", refresh); };
  });

  function applyResponses(responses: { runtime: string | null; response: PeerProposalInboxResponse }[]) {
    const byId = new Map<string, PeerProposalReviewRecord>();

    for (const { runtime, response } of responses) {
      for (const row of [...response.proposals, ...(response.tracked_proposal ? [response.tracked_proposal] : [])]) {
        const previous = byId.get(row.proposal.proposal_id);
        if (!previous?.receipt || row.receipt) {
          byId.set(row.proposal.proposal_id, row);
          proposalOrigins.set(row.proposal.proposal_id, runtime);
        }
      }
    }
    // A failed workshop refresh cannot erase the already observed assignment.
    const selected = rows.find(row => row.proposal.proposal_id === selectedId);
    if (selected && !responses.some(result => result.runtime === selectedRuntime) && !byId.has(selected.proposal.proposal_id)) {
      byId.set(selected.proposal.proposal_id, selected);
      proposalOrigins.set(selected.proposal.proposal_id, selectedRuntime);
    }
    const proposals = [...byId.values()];
    const index = proposals.findIndex(row => row.proposal.proposal_id === selectedId);
    rows = index > 0 ? [proposals[index], ...proposals.slice(0, index), ...proposals.slice(index + 1)] : proposals;
    selectedId = rows[0]?.proposal.proposal_id;
    selectedRuntime = selectedId ? proposalOrigins.get(selectedId) ?? null : null;
    const successful = new Set(responses.map(result => result.runtime));
    cursors = [
      ...cursors.filter(page => !successful.has(page.runtime)),
      ...responses.flatMap(({ runtime, response }) => response.next_cursor ? [{ runtime, cursor: response.next_cursor }] : []),
    ];
    now = Date.now();
  }

  async function action(kind: "approve_and_dispatch" | "deny" | "dispatch") {
    if (!current || busy || !available) return;
    const token = epoch;
    const proposal = current.proposal;
    ++revision;
    busy = true; feedback = null;
    try {
      // Pin paired portals; local workshops use the normal authenticated route.
      const targetWorkshop = proposalWorkshop(proposal.request.target.execution_runtime_id);
      const transport = proposalExecutionTransport(targetWorkshop?.kind, proposal.request.target.execution_runtime_id);
      let response;
      if (kind === "approve_and_dispatch") {
        await actOnPeerProposal(proposal, "approve", transport);
        if (token !== epoch) return;
        // Approval is durable even when provider startup fails. Reflect that
        // boundary immediately so the operator can safely retry dispatch
        // without creating or approving a second assignment.
        rows = rows.map(row => row.proposal.proposal_id === proposal.proposal_id ? { ...row, decision: { proposal_id: proposal.proposal_id, owner_principal_id: proposal.request.owner_principal_id, approved: true } } : row);
        feedback = null;
        response = await actOnPeerProposal(proposal, "dispatch", transport);
      } else {
        response = await actOnPeerProposal(proposal, kind, transport);
      }
      if (token !== epoch) return;
      if (kind === "deny") {
        rows = rows.filter(row => row.proposal.proposal_id !== proposal.proposal_id);
        feedback = "Declined.";
      } else {
        rows = rows.map(row => row.proposal.proposal_id === proposal.proposal_id ? { ...row, binding: response.binding } : row);
        feedback = null;
      }
    } catch (error) {
      if (token === epoch) feedback = error instanceof Error ? error.message : String(error);
    } finally { if (token === epoch) busy = false; }
  }
  async function next(more = false) {
    if (busy || !sessionId) return;
    if (!more && rows.length > 1) { ++revision; rows = [...rows.slice(1), rows[0]]; selectedId = rows[0].proposal.proposal_id; selectedRuntime = proposalOrigins.get(selectedId) ?? null; feedback = null; return; }
    if (!cursors.length) return;
    const token = epoch;
    ++revision;
    busy = true;
    try {
      const pages = [...cursors];
      const settled = await Promise.allSettled(pages.map(async page => ({
        runtime: page.runtime,
        response: await listPeerProposals(sessionId!, page.runtime, page.cursor),
      })));
      if (token !== epoch) return;
      const responses = settled.filter((result): result is PromiseFulfilledResult<{ runtime: string | null; response: PeerProposalInboxResponse }> => result.status === "fulfilled").map(result => result.value);
      if (!responses.length) throw settled.find(result => result.status === "rejected")?.reason ?? new Error("Proposal inbox unavailable");
      for (const page of pages) if (responses.some(result => result.runtime === page.runtime)) pageAfter.set(page.runtime, page.cursor);
      selectedId = undefined;
      applyResponses(responses);
      feedback = settled.some(result => result.status === "rejected") ? "Some workshop requests could not be loaded." : null;
    } catch (error) { if (token === epoch) feedback = String(error); }
    finally { if (token === epoch) busy = false; }
  }
  async function openExecution(kind: "chat" | "project" | "workshop") {
    if (!current || busy) return;
    const row = current;
    const token = epoch;
    try {
      if (kind === "workshop" && executionWorkshop) {
        await workshops.selectWorkshop(executionWorkshop.id);
        return;
      }
      // Session and project ids are scoped to the execution daemon's authority.
      if (!onExecutionWorkshop) return;
      if (kind === "project") {
        const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
        if (token === epoch) await lmeWorkspace.openCodeWorkspace(row.proposal.request.forge_work_id, projectTitle ?? undefined);
      } else {
        const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
        if (token !== epoch) return;
        const id = row.binding?.execution_session.session_id;
        if (!id) return;
        const tab = shellTabs.openChat(id, { title: agentLabel, activate: true });
        if (!tab) await chat.switchSession(id);
      }
    } catch (error) {
      if (token === epoch) feedback = error instanceof Error ? error.message : String(error);
    }
  }
</script>

{#if current && available && progress}
  <section class="delegation-context {mobile ? 'mobile' : ''}" aria-label="Delegated work">
    {#key current.proposal.proposal_id}
      <AgentWorkContextLine title={workTitle} status={progress.status} attention={progress.attention}>
        {#if progress.activity}<p class="activity">{progress.activity}</p>{/if}
        <div class="participants">
          <div class="participant">
            <Bot size={16} aria-hidden="true" />
            <div class="participant-info"><p>{agentLabel}</p><small>{progress.workerStatus} · {workshopLabel}{adopting ? ' · existing work' : ''}</small></div>
            {#if current.binding && onExecutionWorkshop && current.proposal.request.target.runtime === 'medousa'}
              <button type="button" class="text-action" onclick={() => void openExecution('chat')}>Open chat <ArrowUpRight size={13} aria-hidden="true" /></button>
            {/if}
          </div>
          <div class="participant">
            <MessageSquareText size={16} aria-hidden="true" />
            <div class="participant-info"><p>{senderLabel}</p><small>{progress.senderResponsible ? 'Owns the request' : current.handoff ? 'Ownership passed to agent' : 'Requested this work'} · {progress.senderStatus}</small></div>
          </div>
        </div>
        <div class="detail-actions">
          {#if onExecutionWorkshop}
            <button type="button" class="detail-action" onclick={() => void openExecution('project')}><Folder size={13} aria-hidden="true" /> Open project</button>
          {:else if executionWorkshop}
            <button type="button" class="detail-action" onclick={() => void openExecution('workshop')}><ArrowUpRight size={13} aria-hidden="true" /> Open {workshopLabel}</button>
          {/if}
        </div>
        {#if completed}
          <details class="secondary-details"><summary>Agent result</summary><p class="long-text">{completed.result}</p></details>
        {/if}
        {#if current.handoff?.review}
          <p class="review-reason">{current.handoff.review.reason}</p>
        {/if}
        {#if progress.lastActivity && progress.lastActivity !== current.progress?.current_activity}
          <p class="last-activity">Last action: {progress.lastActivity}{progress.lastActivityStatus ? ` · ${progress.lastActivityStatus}` : ''}</p>
        {/if}
        {#if progress.lastUpdate}<p class="last-activity">Updated {progress.lastUpdate}</p>{/if}
        {#if progress.notice}<p class="notice">{progress.notice}</p>{/if}
        <details class="secondary-details"><summary>Assignment</summary><p class="long-text">{current.proposal.request.instructions}</p></details>
        <details class="secondary-details">
          <summary>Shared context and scope</summary>
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
    {/key}
    {#if feedback}<p class="feedback" role="status">{feedback}</p>{/if}
    {#if !completed && !current.binding && progress.needsApproval}
      <div class="approval-actions">
        {#if current.decision?.approved}
          <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || expired} onclick={() => void action('dispatch')}>{busy ? 'Starting…' : 'Start approved work'}</button>
        {:else}
          <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || expired} onclick={() => void action('approve_and_dispatch')}>{busy ? 'Starting…' : adopting ? 'Approve & adopt' : 'Approve & start'}</button>
          <button type="button" class="btn btn-sm variant-ghost-surface" disabled={busy} onclick={() => void action('deny')}>Decline</button>
        {/if}
        {#if expired}<span class="notice">Expired</span>{/if}
      </div>
    {/if}
    {#if rows.length > 1 || cursors.length}
      <div class="request-navigation">
        {#if rows.length > 1}<button type="button" class="text-action" disabled={busy} onclick={() => void next()}>Next request · {rows.length}</button>{/if}
        {#if cursors.length}<button type="button" class="text-action" disabled={busy} onclick={() => void next(true)}>More requests</button>{/if}
      </div>
    {/if}
  </section>
{/if}

<style>
  .delegation-context { min-width: 0; margin: 0 16px 8px; }
  .delegation-context.mobile { margin-inline: 12px; }
  .activity { margin-bottom: 12px; font-size: 13px; color: rgb(var(--theme-text-primary)); }
  .participants { display: flex; flex-direction: column; gap: 12px; }
  .participant { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .participant > :global(svg) { flex-shrink: 0; color: rgb(var(--theme-text-tertiary)); }
  .participant-info { flex: 1; min-width: 0; }
  .participant-info p { font-size: 13px; color: rgb(var(--theme-text-primary)); }
  .participant-info small { display: block; font-size: 12px; color: rgb(var(--theme-text-secondary)); overflow-wrap: anywhere; }
  .text-action { display: inline-flex; align-items: center; gap: 5px; flex-shrink: 0; border: 0; background: transparent; padding: 4px 0; font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  .text-action:hover { color: rgb(var(--theme-text-primary)); }
  .detail-actions, .approval-actions, .request-navigation { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-top: 12px; }
  .detail-action { display: inline-flex; align-items: center; gap: 6px; border: 1px solid rgb(var(--theme-border)); border-radius: 7px; padding: 6px 10px; background: transparent; color: rgb(var(--theme-text-secondary)); font-size: 12px; }
  .detail-action:hover { background: rgb(var(--theme-card-hover)); }
  .secondary-details { margin-top: 12px; color: rgb(var(--theme-text-secondary)); font-size: 12px; }
  .secondary-details summary { cursor: pointer; }
  .long-text { white-space: pre-wrap; overflow-wrap: anywhere; margin-top: 8px; font-size: 13px; }
  dl { margin-top: 8px; overflow-wrap: anywhere; }
  dt { color: rgb(var(--theme-text-tertiary)); margin-top: 8px; }
  .last-activity, .notice, .feedback, .review-reason { margin-top: 8px; color: rgb(var(--theme-text-secondary)); font-size: 12px; overflow-wrap: anywhere; }
  .request-navigation { margin: 4px 8px 0; }
  .approval-actions { margin-left: 8px; }
  @media (max-width: 480px) { .participant { flex-wrap: wrap; } .participant-info { flex-basis: calc(100% - 26px); } .participant .text-action { margin-left: 26px; } }
  @media (pointer: coarse) { .text-action, .detail-action, .secondary-details summary { min-height: 44px; } }
</style>
