<script lang="ts">
  import { untrack } from "svelte";
  import type { Snippet } from "svelte";
  import ChatAgentGroup from "./ChatAgentGroup.svelte";
  import type { PeerProposalControls } from "./peerProposalControls";
  import { chat } from "$lib/stores/chat.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { actOnPeerProposal, listPeerProposals, proposalExecutionTransport } from "$lib/daemon/coordination";
  import type { PeerProposalInboxResponse, PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { isTauri } from "$lib/platform";
  import { requestRemotePeerCompletionSync } from "$lib/remotePeerCompletionSync";
  import { peerRuntimeLabel } from "./peerProgress";

  let { sessionId, mobile = false, children }: { sessionId: string | null; mobile?: boolean; children?: Snippet<[PeerProposalReviewRecord[], PeerProposalControls]> } = $props();
  let rows = $state<PeerProposalReviewRecord[]>([]);
  let cursors = $state<{ runtime: string | null; cursor: string }[]>([]);
  let busy = $state(false);
  let feedback = $state<string | null>(null);
  let now = $state(Date.now());
  let epoch = 0;
  let revision = 0;
  let loadedPages = new Map<string | null, Set<string>>();
  let selectedRuntime: string | null = null;
  let selectedId: string | undefined;
  let proposalOrigins = new Map<string, string | null>();
  let loadedScope: { session: string | null; workshop: string | null; profile: string } | undefined;
  let busyId = $state<string | null>(null);
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
  function proposalWorkshop(runtimeId: string) {
    return workshops.workshops.find(workshop => workshop.pairing?.workshopDeviceId === runtimeId);
  }

  $effect(() => {
    const session = sessionId;
    const workshop = workshops.activeWorkshopId;
    const profile = profileScope;
    const enabled = available;
    const token = ++epoch;
    const scopeChanged = !loadedScope || loadedScope.session !== session || loadedScope.workshop !== workshop
      || Boolean(profile && loadedScope.profile && loadedScope.profile !== profile);
    if (scopeChanged) {
      rows = []; cursors = []; feedback = null;
      loadedPages = new Map(); selectedId = undefined; selectedRuntime = null; proposalOrigins = new Map();
    }
    // Missing health during reconnect is not a change of owner.
    loadedScope = { session, workshop, profile: profile || (scopeChanged ? "" : loadedScope?.profile ?? "") };
    busy = false; busyId = null;
    if (!session || !enabled) return;
    let loading = false;
    const refresh = async () => {
      if (loading || untrack(() => busy) || document.visibilityState === "hidden") return;
      void requestRemotePeerCompletionSync();
      loading = true;
      const requestRevision = revision;
      try {
        const settled = await Promise.allSettled(
          proposalRuntimes.flatMap(runtime => [undefined, ...(loadedPages.get(runtime) ?? [])].map(async after => ({
            runtime, response: await listPeerProposals(session, runtime, after,
              !after && runtime === selectedRuntime ? selectedId : undefined),
          }))),
        );
        const responses = settled
          .filter((result): result is PromiseFulfilledResult<{ runtime: string | null; response: PeerProposalInboxResponse }> => result.status === "fulfilled")
          .map(result => result.value);
        if (token !== epoch || requestRevision !== revision) return;
        const observed = untrack(() => [...rows]);
        const updates = await Promise.allSettled(observed.filter(row => !responses.some(({ response }) =>
          response.proposals.some(item => item.proposal.proposal_id === row.proposal.proposal_id)
          || response.tracked_proposal?.proposal.proposal_id === row.proposal.proposal_id))
          .map(async row => ({ runtime: proposalOrigins.get(row.proposal.proposal_id) ?? null,
            response: await listPeerProposals(session, proposalOrigins.get(row.proposal.proposal_id) ?? null, undefined, row.proposal.proposal_id) })));
        const tracked = updates.filter((result): result is PromiseFulfilledResult<{ runtime: string | null; response: PeerProposalInboxResponse }> => result.status === "fulfilled").map(result => result.value);
        if (!responses.length) throw settled.find(result => result.status === "rejected")?.reason ?? new Error("Proposal inbox unavailable");
        if (token === epoch && requestRevision === revision) {
          applyResponses(responses, tracked);
          feedback = settled.some(result => result.status === "rejected") || updates.some(result => result.status === "rejected")
            ? "Some agent progress could not be refreshed. Showing the last known activity." : null;
        }
      } catch (error) {
        if (token === epoch && requestRevision === revision && untrack(() => rows.length > 0)) feedback = String(error);
      } finally { loading = false; }
    };
    void refresh();
    const timer = setInterval(() => { now = Date.now(); void refresh(); }, 15_000);
    document.addEventListener("visibilitychange", refresh);
    return () => { ++epoch; clearInterval(timer); document.removeEventListener("visibilitychange", refresh); };
  });

  function applyResponses(responses: { runtime: string | null; response: PeerProposalInboxResponse }[], tracked: { runtime: string | null; response: PeerProposalInboxResponse }[] = []) {
    const byId = new Map(rows.map(row => [row.proposal.proposal_id, row]));

    for (const { runtime, response } of [...responses, ...tracked]) {
      for (const row of [...response.proposals, ...(response.tracked_proposal ? [response.tracked_proposal] : [])]) {
        const previous = byId.get(row.proposal.proposal_id);
        if (!previous?.receipt || row.receipt) {
          byId.set(row.proposal.proposal_id, row);
          proposalOrigins.set(row.proposal.proposal_id, runtime);
        }
      }
    }
    // Keep every observed agent in spawn order, including terminal results.
    rows = [...byId.values()].sort((a, b) => Date.parse(a.proposal.request.context.created_at) - Date.parse(b.proposal.request.context.created_at));
    selectedId = rows[0]?.proposal.proposal_id;
    selectedRuntime = selectedId ? proposalOrigins.get(selectedId) ?? null : null;
    const successful = new Set(responses.map(result => result.runtime));
    cursors = [...new Map([
      ...cursors.filter(page => !successful.has(page.runtime)),
      ...responses.flatMap(({ runtime, response }) => response.next_cursor && !loadedPages.get(runtime)?.has(response.next_cursor) ? [{ runtime, cursor: response.next_cursor }] : []),
    ].map(page => [JSON.stringify([page.runtime, page.cursor]), page])).values()];
    now = Date.now();
  }

  async function action(current: PeerProposalReviewRecord, kind: "approve_and_dispatch" | "deny" | "dispatch") {
    if (busy || !available) return;
    const token = epoch;
    const proposal = current.proposal;
    ++revision;
    busy = true; busyId = proposal.proposal_id; feedback = null;
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
    } finally { if (token === epoch) { busy = false; busyId = null; } }
  }
  async function next() {
    if (busy || !sessionId) return;
    if (!cursors.length || !available) return;
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
      for (const page of pages) if (responses.some(result => result.runtime === page.runtime)) {
        const loaded = loadedPages.get(page.runtime) ?? new Set<string>();
        loaded.add(page.cursor); loadedPages.set(page.runtime, loaded);
      }
      selectedId = undefined;
      applyResponses(responses);
      feedback = settled.some(result => result.status === "rejected") ? "Some workshop requests could not be loaded." : null;
    } catch (error) { if (token === epoch) feedback = String(error); }
    finally { if (token === epoch) { busy = false; busyId = null; } }
  }
  async function openExecution(row: PeerProposalReviewRecord, kind: "chat" | "project" | "workshop") {
    if (busy) return;
    const executionWorkshop = proposalWorkshop(row.proposal.request.target.execution_runtime_id);
    const onExecutionWorkshop = row.proposal.request.target.authority_id === connection.health?.runtime?.authority_id || executionWorkshop?.id === workshops.activeWorkshopId;
    const projectTitle = onExecutionWorkshop ? (undertakings.active?.workId === row.proposal.request.forge_work_id ? undertakings.active.title : undertakings.items.find(item => item.id === row.proposal.request.forge_work_id)?.title) : null;
    const agentLabel = peerRuntimeLabel(row.proposal.request.target.runtime);
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
  const controls = $derived<PeerProposalControls>({ now, available, busy, busyId, feedback,
    hasMore: cursors.length > 0, loadMore: next, action, openExecution });
</script>

{#if children}
  {@render children(rows, controls)}
{:else if rows.length}
  <section class="delegation-context" class:mobile aria-label="Delegated work">
    <ChatAgentGroup group={{ proposals: rows, workers: [] }} {controls} />
  </section>
{/if}
{#if feedback}<p class="feedback" role="status">{feedback}</p>{/if}
{#if cursors.length}
  <button type="button" class="more-requests" disabled={busy || !available} onclick={() => void next()}>More requests</button>
{/if}

<style>
  .delegation-context { min-width: 0; margin: 0 16px 8px; }
  .delegation-context.mobile { margin-inline: 12px; }
  .feedback { margin: 8px 16px; color: rgb(var(--theme-text-secondary)); font-size: 12px; overflow-wrap: anywhere; }
  .more-requests { display: block; margin: 8px 16px; border: 0; background: transparent; padding: 6px 0; font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  @media (pointer: coarse) { .more-requests { min-height: 44px; } }
</style>
