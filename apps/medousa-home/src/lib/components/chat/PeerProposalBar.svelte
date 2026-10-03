<script lang="ts">
  import { untrack } from "svelte";
  import { actOnPeerProposal, listPeerProposals, proposalExecutionTransport } from "$lib/daemon/coordination";
  import type { PeerProposalInboxResponse, PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { isTauri } from "$lib/platform";
  import { requestRemotePeerCompletionSync } from "$lib/remotePeerCompletionSync";
  import { peerProgressPresentation, peerRuntimeLabel } from "./peerProgress";

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
  const progress = $derived(current ? peerProgressPresentation(current, now) : null);
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
        feedback = "Approved. Starting the agent…";
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
        feedback = adopting ? "Existing work adopted. Medousa is tracking it." : "Work accepted. Medousa is tracking it.";
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
</script>

{#if current && available}
  <section class="{mobile ? 'mx-3' : 'mx-4'} mb-2 rounded-xl border border-primary-400/25 bg-surface-900 p-3" aria-label="Agent delegation approval">
    <div class="flex items-center justify-between gap-2">
      <p class="text-xs font-medium text-content-link">{completed ? 'Agent result · verified terminal' : current.binding ? progress?.headline : current.decision?.approved ? 'Approved delegation' : 'Delegate work · needs your approval'}</p>
      <div class="flex gap-2">
        {#if rows.length > 1}<button type="button" class="text-xs text-content-secondary" disabled={busy} onclick={() => void next()}>Next request</button>{/if}
        {#if cursors.length}<button type="button" class="text-xs text-content-secondary" disabled={busy} onclick={() => void next(true)}>More requests</button>{/if}
      </div>
    </div>
    <p class="mt-1 text-sm text-content-primary">{peerRuntimeLabel(current.proposal.request.target.runtime)} · {proposalWorkshop(current.proposal.request.target.execution_runtime_id)?.label ?? workshops.activeLabel}{adopting ? ' · existing work' : ''}</p>
    <p class="mt-1 whitespace-pre-wrap text-sm text-content-secondary">{current.proposal.request.instructions}</p>
    {#if completed}
      <div class="mt-2 rounded-lg border border-primary-400/15 bg-surface-950/60 p-2">
        <p class="text-xs font-medium text-content-primary">{progress?.headline}</p>
        <p class="mt-1 max-h-40 overflow-y-auto whitespace-pre-wrap text-sm text-content-secondary">{completed.result}</p>
      </div>
    {/if}
    {#if current.binding && !completed && progress}
      <div class="mt-2 space-y-1 text-xs text-content-secondary" role="status" aria-live="polite">
        {#if progress.activity}<p class="text-sm text-content-primary">{progress.activity}</p>{/if}
        {#if progress.lastActivity && progress.lastActivity !== current.progress?.current_activity}
          <p>Last action: {progress.lastActivity}{progress.lastActivityStatus === 'failed' ? ' · failed' : progress.lastActivityStatus === 'blocked' ? ' · blocked' : progress.lastActivityStatus === 'succeeded' ? ' · succeeded' : ''}</p>
        {/if}
        {#if progress.lastUpdate}<p>Last update {progress.lastUpdate}</p>{/if}
        {#if progress.notice}<p>{progress.notice}</p>{/if}
      </div>
    {/if}
    <details class="mt-2 text-xs text-content-secondary">
      <summary class="cursor-pointer">Review shared context and scope</summary>
      <dl class="mt-2 space-y-1 break-all">
        <dt>Channel</dt><dd>{current.proposal.request.channel.channel_id}</dd>
        <dt>Work item</dt><dd>{current.proposal.request.forge_work_id}</dd>
        <dt>Execution workshop</dt><dd>{current.proposal.request.target.execution_runtime_id} · {current.proposal.request.target.authority_id}</dd>
        {#if current.proposal.request.existing_agent_session_id}<dt>Existing agent session</dt><dd>{current.proposal.request.existing_agent_session_id}</dd>{/if}
        {#if current.binding}<dt>Agent custody</dt><dd>{current.binding.agent_session_id}</dd>{/if}
        <dt>Shared conversation ranges</dt>
        {#each current.proposal.request.context.sources as source}
          <dd>{source.selection.session.session_id} · entries {(source.selection.after_entry_seq ?? 0) + 1}–{source.selection.through_entry_seq} · {source.selection_digest}</dd>
        {/each}
        <dt>Owner continuation</dt><dd>{current.proposal.continue_owner ? 'Report the result and, if you already requested it, prepare one follow-up handoff. Every launch still needs approval.' : 'Not approved by this proposal.'}</dd>
        <dt>Expires</dt><dd>{new Date(current.proposal.expires_at).toLocaleString()}{expired ? ' · expired' : ''}</dd>
        <dt>Proposal</dt><dd>{current.proposal.proposal_id}</dd>
      </dl>
    </details>
    {#if feedback}<p class="mt-2 text-xs text-content-secondary" role="status">{feedback}</p>{/if}
    <div class="mt-3 flex gap-2">
      {#if completed}
        <span class="text-xs text-content-secondary">Medousa received the terminal result.</span>
      {:else if current.binding}
        <span class="text-xs text-content-secondary">The final result will appear here when Medousa verifies it.</span>
      {:else if current.decision?.approved}
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || expired} onclick={() => void action('dispatch')}>Start approved work</button>
      {:else}
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || expired} onclick={() => void action('approve_and_dispatch')}>{adopting ? 'Approve & adopt' : 'Approve & start'}</button>
        <button type="button" class="btn btn-sm variant-ghost-surface" disabled={busy} onclick={() => void action('deny')}>Decline</button>
      {/if}
    </div>
  </section>
{/if}
