<script lang="ts">
  import { untrack } from "svelte";
  import { actOnPeerProposal, listPeerProposals, proposalExecutionTransport } from "$lib/daemon/coordination";
  import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { isTauri } from "$lib/platform";

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
  const profileScope = $derived(connection.health?.active_profile_id ?? "");
  const available = $derived(isTauri() && connection.online && !workshops.switching && Boolean(connection.health?.runtime?.advertised_capabilities.includes("coordination.operator_proposals.v1")));
  const expired = $derived(current ? Date.parse(current.proposal.expires_at) <= now : false);

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
      loading = true;
      const requestRevision = revision;
      try {
        const response = await listPeerProposals(session, undefined, pageAfter);
        if (token === epoch && requestRevision === revision) {
          const index = response.proposals.findIndex(row => row.proposal.proposal_id === selectedId);
          rows = index > 0 ? [...response.proposals.slice(index), ...response.proposals.slice(0, index)] : response.proposals;
          cursor = response.next_cursor ?? null; now = Date.now();
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

  async function action(kind: "approve" | "deny" | "dispatch") {
    if (!current || busy || !available) return;
    const token = epoch;
    const proposal = current.proposal;
    ++revision;
    busy = true; feedback = null;
    try {
      // Pin paired portals; local workshops use the normal authenticated route.
      const transport = proposalExecutionTransport(workshops.activeWorkshop?.kind, proposal.request.target.execution_runtime_id);
      await actOnPeerProposal(proposal, kind, transport);
      if (token !== epoch) return;
      if (kind === "approve") {
        rows = rows.map(row => row.proposal.proposal_id === proposal.proposal_id ? { ...row, decision: { proposal_id: proposal.proposal_id, owner_principal_id: proposal.request.owner_principal_id, approved: true } } : row);
        feedback = "Approved. Start the work when you’re ready.";
      } else {
        rows = rows.filter(row => row.proposal.proposal_id !== proposal.proposal_id);
        feedback = kind === "deny" ? "Declined." : "Work accepted by the agent.";
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
      <p class="text-xs font-medium text-content-link">{current.decision?.approved ? 'Approved delegation' : 'Delegate work · needs your approval'}</p>
      <div class="flex gap-2">
        {#if rows.length > 1}<button type="button" class="text-xs text-content-secondary" disabled={busy} onclick={() => void next()}>Next request</button>{/if}
        {#if cursor}<button type="button" class="text-xs text-content-secondary" disabled={busy} onclick={() => void next(true)}>More requests</button>{/if}
      </div>
    </div>
    <p class="mt-1 text-sm text-content-primary">{current.proposal.request.target.runtime} · {workshops.activeLabel}</p>
    <p class="mt-1 whitespace-pre-wrap text-sm text-content-secondary">{current.proposal.request.instructions}</p>
    <details class="mt-2 text-xs text-content-secondary">
      <summary class="cursor-pointer">Review shared context and scope</summary>
      <dl class="mt-2 space-y-1 break-all">
        <dt>Channel</dt><dd>{current.proposal.request.channel.channel_id}</dd>
        <dt>Work item</dt><dd>{current.proposal.request.forge_work_id}</dd>
        <dt>Execution workshop</dt><dd>{current.proposal.request.target.execution_runtime_id} · {current.proposal.request.target.authority_id}</dd>
        <dt>Shared conversation ranges</dt>
        {#each current.proposal.request.context.sources as source}
          <dd>{source.selection.session.session_id} · entries {(source.selection.after_entry_seq ?? 0) + 1}–{source.selection.through_entry_seq} · {source.selection_digest}</dd>
        {/each}
        <dt>Owner continuation</dt><dd>{current.proposal.continue_owner ? 'One result-only turn in this chat; no follow-up tools.' : 'Not approved by this proposal.'}</dd>
        <dt>Expires</dt><dd>{new Date(current.proposal.expires_at).toLocaleString()}{expired ? ' · expired' : ''}</dd>
        <dt>Proposal</dt><dd>{current.proposal.proposal_id}</dd>
      </dl>
    </details>
    {#if feedback}<p class="mt-2 text-xs text-content-secondary" role="status">{feedback}</p>{/if}
    <div class="mt-3 flex gap-2">
      {#if current.decision?.approved}
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || expired} onclick={() => void action('dispatch')}>Start approved work</button>
      {:else}
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || expired} onclick={() => void action('approve')}>Approve</button>
        <button type="button" class="btn btn-sm variant-ghost-surface" disabled={busy} onclick={() => void action('deny')}>Decline</button>
      {/if}
    </div>
  </section>
{/if}
