<script lang="ts">
  import { renderMarkdown } from "$lib/markdown";
  import { connection } from "$lib/stores/connection.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
  import type { PeerProposalControls } from "./peerProposalControls";
  import { peerHandoffPresentation, peerWorkTitle } from "./peerHandoffPresentation";

  let { row, controls }: { row: PeerProposalReviewRecord; controls: PeerProposalControls } = $props();
  const progress = $derived(peerHandoffPresentation(row, controls.now));
  const error = $derived(controls.actionErrors?.[row.proposal.proposal_id]);
  const executionWorkshop = $derived(workshops.workshops.find(workshop => workshop.pairing?.workshopDeviceId === row.proposal.request.target.execution_runtime_id));
  const local = $derived(row.proposal.request.target.authority_id === connection.health?.runtime?.authority_id || executionWorkshop?.id === workshops.activeWorkshopId);
  const busy = $derived(controls.busyId === row.proposal.proposal_id);
  const needsApproval = $derived(!row.receipt && !row.binding && progress.needsApproval);
</script>

<section class="task-detail" aria-label="Task decision and result">
  <p class="eyebrow">{needsApproval ? 'Your decision' : progress.status}</p>
  <h2>{peerWorkTitle(row.proposal.request.instructions)}</h2>
  <p class="scope">{needsApproval ? 'Approval allows this agent to start the requested work. It does not approve, apply or publish its eventual changes.' : 'A returned result is not proof of acceptance, merging or publication.'}</p>
  <section><h3>Requested work</h3><p class="copy">{row.proposal.request.instructions}</p></section>
  {#if row.receipt}
    <section><h3>Result</h3><div class="markdown-body">{@html renderMarkdown(row.receipt.result)}</div></section>
  {:else if progress.activity}
    <section><h3>Latest update</h3><p>{progress.activity}</p></section>
  {/if}
  {#if row.handoff?.review}<section><h3>Review</h3><p class="copy">{row.handoff.review.reason}</p></section>{/if}
  {#if progress.nextActor}<p class="scope">Next: {progress.nextActor}</p>{/if}
  {#if progress.notice}<p role="status">{progress.notice}</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  {#if needsApproval}
    <div class="actions">
      {#if row.decision?.approved}
        <button type="button" disabled={controls.busy || !controls.available} onclick={() => void controls.action(row, 'dispatch')}>{busy ? 'Starting…' : 'Start approved work'}</button>
      {:else}
        <button type="button" disabled={controls.busy || !controls.available} onclick={() => void controls.action(row, 'approve_and_dispatch')}>{busy ? 'Starting…' : row.proposal.request.existing_agent_session_id ? 'Approve & adopt' : 'Approve & start'}</button>
        <button type="button" class="secondary" disabled={controls.busy || !controls.available} onclick={() => void controls.action(row, 'deny')}>Decline</button>
      {/if}
    </div>
  {/if}
  {#if local && row.binding}<button type="button" class="secondary" onclick={() => void controls.openExecution(row, 'chat')}>Open agent conversation ↗</button>
  {:else if executionWorkshop && !local}<button type="button" class="secondary" onclick={() => void controls.openExecution(row, 'workshop')}>Open {executionWorkshop.label} ↗</button>{/if}
</section>

<style>
  .task-detail { min-width: 0; }
  .eyebrow { color: rgb(var(--theme-warning)); font-size: 12px; }
  h2 { font-size: 20px; margin: 8px 0; overflow-wrap: anywhere; }
  h3 { font-size: 12px; color: rgb(var(--theme-text-secondary)); margin-bottom: 8px; }
  section section { border-top: 1px solid rgb(var(--theme-border)); padding-block: 18px; }
  p, .markdown-body { font-size: 13px; line-height: 1.6; overflow-wrap: anywhere; }
  .copy { white-space: pre-wrap; }
  .scope { color: rgb(var(--theme-text-secondary)); margin-block: 12px; }
  .actions { display: flex; flex-wrap: wrap; gap: 10px; margin-block: 18px; }
  button { padding: 9px 14px; border-radius: 8px; background: rgb(var(--theme-action)); color: rgb(var(--on-primary)); border: 1px solid rgb(var(--theme-border)); cursor: pointer; }
  button.secondary { background: transparent; color: rgb(var(--theme-text)); }
  button:disabled { opacity: .5; cursor: default; }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 3px; }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
