<script lang="ts">
  import type { PeerProposalReviewRecord } from '$lib/types/generated/daemon_api';
  import type { PeerProposalControls } from './peerProposalControls';
  import { connection } from '$lib/stores/connection.svelte';
  import { workshops } from '$lib/stores/workshops.svelte';
  import { peerHandoffPresentation, peerWorkTitle } from './peerHandoffPresentation';
  import { peerRuntimeLabel } from './peerProgress';
  import AgentTaskDetail from './AgentTaskDetail.svelte';
  import AgentExchangeHistory from './AgentExchangeHistory.svelte';
  import ExchangeMessage from './ExchangeMessage.svelte';
  let { row, controls }: { row: PeerProposalReviewRecord; controls: PeerProposalControls } = $props();
  const progress = $derived(peerHandoffPresentation(row, controls.now));
  const participant = $derived(peerRuntimeLabel(row.proposal.request.target.runtime));
  const executionWorkshop = $derived(workshops.workshops.find(workshop => workshop.pairing?.workshopDeviceId === row.proposal.request.target.execution_runtime_id));
  const local = $derived(row.proposal.request.target.authority_id === connection.health?.runtime?.authority_id || executionWorkshop?.id === workshops.activeWorkshopId);
  const sessionId = $derived(local ? row.binding?.execution_session.session_id : null);
</script>
<h1>{peerWorkTitle(row.proposal.request.instructions)}</h1>
<p class="status">{progress.status}{progress.nextActor ? ` · Next: ${progress.nextActor}` : ''}</p>
{#if progress.notice}<p role="status" class="notice">{progress.notice}</p>{/if}
{#if progress.needsApproval}
  <AgentTaskDetail {row} {controls} />
{:else}
  {#if sessionId}
    {#key sessionId}<AgentExchangeHistory {sessionId} authorityId={row.binding!.execution_session.authority_id} {participant} />{/key}
    <details><summary>Original handoff</summary><ExchangeMessage label="Medousa · Assignment" text={row.proposal.request.instructions} sender /></details>
  {:else}
    <p class="notice">Recorded handoff exchange{!local ? ' · Full conversation remains on the execution workshop' : ''}</p>
    <ExchangeMessage label="Medousa · Assignment" text={row.proposal.request.instructions} sender />
  {/if}
  {#if row.receipt}<ExchangeMessage label={`${participant} · Returned result`} text={row.receipt.result} />
  {:else if progress.activity}<ExchangeMessage label={`${participant} · Latest update`} text={progress.activity} />{/if}
  {#if row.handoff?.review}<ExchangeMessage label="Medousa · Review" text={row.handoff.review.reason} />{/if}
  <details><summary>Assignment details & navigation</summary><AgentTaskDetail {row} {controls} /></details>
  <p class="notice">A returned result is not approval, application or publication.</p>
{/if}
{#if controls.feedback}<p role="status">{controls.feedback}</p>{/if}
<style>
  h1 { font-size: 20px; line-height: 1.4; overflow-wrap: anywhere; }
  .status { color: rgb(var(--theme-text-secondary)); font-size: 12px; margin: 8px 0 24px; }
  .notice { color: rgb(var(--theme-text-tertiary)); font-size: 12px; line-height: 1.6; margin: 16px 0 24px; }
  details { color: rgb(var(--theme-text-secondary)); margin-block: 20px; font-size: 12px; }
  summary { cursor: pointer; padding-block: 10px; }
</style>
