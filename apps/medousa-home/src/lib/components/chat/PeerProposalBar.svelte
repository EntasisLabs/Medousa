<script lang="ts">
  import { untrack } from "svelte";
  import { actOnPeerProposal, listPeerProposals, proposalExecutionTransport } from "$lib/daemon/coordination";
  import type { PeerProposalInboxResponse, PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { isTauri } from "$lib/platform";
  import { requestRemotePeerCompletionSync } from "$lib/remotePeerCompletionSync";

  let { sessionId, mobile = false }: { sessionId: string | null; mobile?: boolean } = $props();
  let rows = $state<PeerProposalReviewRecord[]>([]);
  let cursor = $state<string | null>(null);
  let busy = $state(false);
  let feedback = $state<string | null>(null);
  let now = $state(Date.now());
  let epoch = 0;
  let revision = 0;
  let pageAfter: string | undefined;
  let selectedId: string | undefined;
  const current = $derived(rows[0] ?? null);
  const completed = $derived(current?.receipt ?? null);
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
    rows = []; cursor = null; feedback = null; busy = false;
    pageAfter = undefined; selectedId = undefined;
    if (!session || !enabled) return;
    let loading = false;
    const refresh = async () => {
      if (loading || untrack(() => busy) || document.visibilityState === "hidden") return;
      void requestRemotePeerCompletionSync();
      loading = true;
      const requestRevision = revision;
      try {
        const settled = await Promise.allSettled(
          proposalRuntimes.map(runtime => listPeerProposals(session, runtime, pageAfter)),
        );
        const responses = settled
          .filter((result): result is PromiseFulfilledResult<PeerProposalInboxResponse> => result.status === "fulfilled")
          .map(result => result.value);
        if (!responses.length) throw settled.find(result => result.status === "rejected")?.reason ?? new Error("Proposal inbox unavailable");
        if (token === epoch && requestRevision === revision) {
          const proposals = responses
            .flatMap(response => response.proposals)
            .filter((row, index, all) => all.findIndex(candidate => candidate.proposal.proposal_id === row.proposal.proposal_id) === index);
          const index = proposals.findIndex(row => row.proposal.proposal_id === selectedId);
          rows = index > 0 ? [...proposals.slice(index), ...proposals.slice(0, index)] : proposals;
          cursor = null; now = Date.now();
          if (!rows.length && pageAfter) pageAfter = undefined;
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
    if (!more && rows.length > 1) { rows = [...rows.slice(1), rows[0]]; selectedId = rows[0].proposal.proposal_id; feedback = null; return; }
    if (!cursor) return;
    const token = epoch;
    ++revision;
    busy = true;
    try {
      const after = cursor;
      const response = await listPeerProposals(sessionId, undefined, after);
      if (token === epoch) { pageAfter = after; selectedId = undefined; rows = response.proposals; cursor = response.next_cursor ?? null; }
    } catch (error) { if (token === epoch) feedback = String(error); }
    finally { if (token === epoch) busy = false; }
  }
</script>

{#if current && available}
  <section class="{mobile ? 'mx-3' : 'mx-4'} mb-2 rounded-xl border border-primary-400/25 bg-surface-900 p-3" aria-label="Agent delegation approval">
    <div class="flex items-center justify-between gap-2">
      <p class="text-xs font-medium text-content-link">{completed ? 'Agent result · verified terminal' : current.binding ? 'Medousa is tracking this work' : current.decision?.approved ? 'Approved delegation' : 'Delegate work · needs your approval'}</p>
      <div class="flex gap-2">
        {#if rows.length > 1}<button type="button" class="text-xs text-content-secondary" disabled={busy} onclick={() => void next()}>Next request</button>{/if}
        {#if cursor}<button type="button" class="text-xs text-content-secondary" disabled={busy} onclick={() => void next(true)}>More requests</button>{/if}
      </div>
    </div>
    <p class="mt-1 text-sm text-content-primary">{current.proposal.request.target.runtime} · {proposalWorkshop(current.proposal.request.target.execution_runtime_id)?.label ?? workshops.activeLabel}{adopting ? ' · existing work' : ''}</p>
    <p class="mt-1 whitespace-pre-wrap text-sm text-content-secondary">{current.proposal.request.instructions}</p>
    {#if completed}
      <div class="mt-2 rounded-lg border border-primary-400/15 bg-surface-950/60 p-2">
        <p class="text-xs font-medium text-content-primary">{completed.outcome}</p>
        <p class="mt-1 max-h-40 overflow-y-auto whitespace-pre-wrap text-sm text-content-secondary">{completed.result}</p>
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
        <span class="text-xs text-content-secondary">Waiting for the verified terminal result…</span>
      {:else if current.decision?.approved}
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || expired} onclick={() => void action('dispatch')}>Start approved work</button>
      {:else}
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || expired} onclick={() => void action('approve_and_dispatch')}>{adopting ? 'Approve & adopt' : 'Approve & start'}</button>
        <button type="button" class="btn btn-sm variant-ghost-surface" disabled={busy} onclick={() => void action('deny')}>Decline</button>
      {/if}
    </div>
  </section>
{/if}
