<script lang="ts">
  import { ChevronRight, GitBranch } from "@lucide/svelte";
  import type { ChatAgentGroup as Group } from "$lib/utils/chatAgentGroups";
  import type { PeerProposalControls } from "./peerProposalControls";
  import PeerProposalRow from "./PeerProposalRow.svelte";
  import ChatSubagentRow from "./ChatSubagentRow.svelte";
  import { peerHandoffPresentation } from "./peerHandoffPresentation";

  let { group, controls, id, compact = false, onOpenSubagent, onStopSubagent }: {
    group: Group;
    controls?: PeerProposalControls;
    id?: string;
    compact?: boolean;
    onOpenSubagent?: (workId: string) => void;
    onStopSubagent?: (workId: string) => void;
  } = $props();
  const count = $derived(group.proposals.length + group.workers.length);
  const rows = $derived([
    ...group.proposals.map(row => ({ kind: "proposal" as const, id: row.proposal.proposal_id, row })),
    ...group.workers.map(row => ({ kind: "worker" as const, id: row.workId, row })),
  ]);
  const summary = $derived.by(() => {
    let working = 0, review = 0, accepted = 0, completed = 0, attention = 0, waiting = 0;
    for (const row of group.proposals) {
      const progress = peerHandoffPresentation(row, controls?.now ?? Date.now());
      if (progress.attention || progress.needsApproval) attention += 1;
      else if (row.handoff?.state === "awaiting_sender_review") review += 1;
      else if (row.handoff?.review?.verdict === "accept" && row.handoff.state === "accepted") accepted += 1;
      else if (row.receipt) completed += 1;
      else if (row.progress?.state === "running" || row.handoff?.state === "working") working += 1;
      else waiting += 1;
    }
    for (const row of group.workers) {
      if (row.attention) attention += 1;
      else if (row.terminal) completed += 1;
      else if (row.streaming) working += 1;
      else waiting += 1;
    }
    return [[working, "working"], [review, "ready for review"], [accepted, "accepted"],
      [completed, "completed"], [attention, attention === 1 ? "needs attention" : "need attention"], [waiting, "waiting"]]
      .filter(([value]) => value).map(([value, label]) => `${value} ${label}`).join(" · ");
  });
</script>

{#snippet agentRow(item: (typeof rows)[number])}
  {#if item.kind === "proposal"}
    {#if controls}<PeerProposalRow row={item.row} {controls} />{/if}
  {:else}
    <ChatSubagentRow row={item.row} {compact} onOpen={() => onOpenSubagent?.(item.row.workId)} onStop={onStopSubagent ? () => onStopSubagent(item.row.workId) : undefined} />
  {/if}
{/snippet}

{#if count}
  <details {id} class="agent-group" open aria-label="Agent activity">
    <summary class="group-header">
      <GitBranch size={17} aria-hidden="true" />
      <span class="group-heading"><strong>{count} agent{count === 1 ? "" : "s"}</strong><span class="group-status" aria-live="polite">{summary}</span></span>
      <ChevronRight size={14} class="group-chevron" aria-hidden="true" />
    </summary>
    <div class="agent-rows">
      {#each rows.slice(0, 4) as item (`${item.kind}:${item.id}`)}{@render agentRow(item)}{/each}
      {#if rows.length > 4}
        <details class="more-agents">
          <summary><ChevronRight size={13} class="more-chevron" aria-hidden="true" />{rows.length - 4} more agent{rows.length === 5 ? "" : "s"}</summary>
          {#each rows.slice(4) as item (`${item.kind}:${item.id}`)}{@render agentRow(item)}{/each}
        </details>
      {/if}
    </div>
  </details>
{/if}

<style>
  .agent-group { min-width: 0; margin-block: 14px; border: 1px solid rgb(var(--theme-border) / .75); border-radius: 10px; background: rgb(var(--theme-card)); overflow: hidden; }
  .group-header { display: flex; align-items: center; gap: 10px; padding: 12px 14px; min-height: 56px; list-style: none; cursor: pointer; color: rgb(var(--theme-text-secondary)); }
  .group-header::-webkit-details-marker { display: none; }
  .group-header:hover { background: rgb(var(--theme-card-hover)); }
  .group-header > :global(svg) { flex-shrink: 0; }
  .group-heading { min-width: 0; flex: 1; }
  strong { display: block; font-size: 13px; font-weight: 500; color: rgb(var(--theme-text-primary)); }
  .group-status { display: block; margin-top: 2px; font-size: 12px; overflow-wrap: anywhere; }
  .group-header :global(.group-chevron) { transition: transform 150ms; }
  details[open] > .group-header :global(.group-chevron) { transform: rotate(90deg); }
  .agent-rows { min-width: 0; }
  .more-agents { border-top: 1px solid rgb(var(--theme-border) / .6); }
  .more-agents > summary { display: flex; align-items: center; gap: 8px; padding: 10px 14px; min-height: 40px; list-style: none; cursor: pointer; font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  .more-agents > summary::-webkit-details-marker { display: none; }
  .more-agents[open] > summary :global(.more-chevron) { transform: rotate(90deg); }
  @media (pointer: coarse) { .more-agents > summary { min-height: 44px; } }
  @media (prefers-reduced-motion: reduce) { .group-header :global(.group-chevron) { transition: none; } }
</style>
