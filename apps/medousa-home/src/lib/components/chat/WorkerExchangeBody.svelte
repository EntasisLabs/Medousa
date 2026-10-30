<script lang="ts">
  import { untrack } from 'svelte';
  import type { ToolRunState } from '$lib/types/chat';
  import { workerTranscripts } from '$lib/work/workerTranscripts.svelte';
  import { toolRunsFromWorkerActivity } from '$lib/utils/subagentRows';
  import ExchangeMessage from './ExchangeMessage.svelte';
  import ToolRunChips from './ToolRunChips.svelte';
  let { workId }: { workId: string } = $props();
  let failed = $state(false);
  const task = $derived(workerTranscripts.transcriptFor(workId));
  // A preview is not a final result. Prefer complete persisted content when
  // available, and never let an older short excerpt eclipse a longer stream.
  const answer = $derived(task?.historyLoaded ? task.resultText?.trim() || task.output.trim() || ''
    : (task?.output?.trim().length ?? 0) > (task?.resultText?.trim().length ?? 0)
      ? task!.output.trim() : task?.resultText?.trim() || '');
  const timeline = $derived.by(() => {
    const followups = (task?.steerMessages ?? []).map((message, index) => ({ kind: 'followup' as const, message, at: Date.parse(message.at), index }));
    const tools = toolRunsFromWorkerActivity(task?.toolRuns ?? []).map((run, index) => ({ kind: 'tool' as const, run, at: Date.parse(task!.toolRuns[index].started_at), index: followups.length + index }));
    const ordered = [...followups, ...tools].sort((a, b) =>
      (Number.isFinite(a.at) ? a.at : Infinity) - (Number.isFinite(b.at) ? b.at : Infinity) || a.index - b.index);
    const groups: Array<(typeof followups)[number] | { kind: 'tools'; runs: ToolRunState[] }> = [];
    for (const entry of ordered) {
      if (entry.kind === 'followup') groups.push(entry);
      else {
        const previous = groups.at(-1);
        if (previous?.kind === 'tools') previous.runs.push(entry.run);
        else groups.push({ kind: 'tools', runs: [entry.run] });
      }
    }
    return groups;
  });
  $effect(() => {
    const id = workId;
    let current = true;
    failed = false;
    untrack(() => { void workerTranscripts.refresh(id).then(result => { if (current) failed = !result; }); });
    const timer = setInterval(() => {
      if (current && !document.hidden && !untrack(() => workerTranscripts.transcriptFor(id)?.terminal)) {
        void workerTranscripts.refresh(id, true);
      }
    }, 5000);
    return () => { current = false; clearInterval(timer); };
  });
</script>
{#if task}
  <h1>{task.title}</h1>
  <p class="status">{task.streaming ? 'Working' : task.terminal ? 'Session ended' : task.statusLine || 'Status unavailable'}</p>
  {#if !task.historyLoaded}<p class="notice" role="status">Only the card preview is available. Complete worker history could not be loaded.</p>{/if}
  {#if task.retentionNote}<p class="notice">{task.retentionNote}</p>{/if}
  <ExchangeMessage label={task.historyLoaded ? 'Medousa · Assignment' : 'Medousa · Assignment summary'} text={task.taskPrompt || task.title} sender />
  {#each timeline as entry}
    {#if entry.kind === 'followup'}
      <ExchangeMessage label="Medousa · Follow-up" text={entry.message.text} timestamp={entry.message.at} sender />
    {:else}
      <section aria-label="Recorded worker tools">
        <ToolRunChips runs={entry.runs} inspectorPresentation="inline" />
      </section>
    {/if}
  {/each}
  {#if answer}<ExchangeMessage label={task.streaming ? 'Agent · Latest output' : 'Agent · Result'} text={answer} />
  {:else}<p role="status">{task.streaming ? 'The agent is working. Its output will appear here.' : 'No written result was recorded.'}</p>{/if}
  {#if task.error}<p role="alert">{task.error}</p>{/if}
  <details><summary>Technical details</summary><p>Work: {workId}</p>{#if task.sessionId}<p>Parent session: {task.sessionId}</p>{/if}{#if task.executionRuntimeId}<p>Execution runtime: {task.executionRuntimeId}</p>{/if}{#if task.model}<p>Model: {task.model}</p>{/if}</details>
  <p class="notice">A returned result does not mean changes were approved, applied or published.</p>
{:else}
  <p role="status">{failed ? 'This task could not be loaded. Return to our chat and reopen it to try again.' : 'Loading exchange…'}</p>
{/if}
<style>
  h1 { font-size: 20px; line-height: 1.4; overflow-wrap: anywhere; }
  .status { color: rgb(var(--theme-text-secondary)); font-size: 12px; margin: 8px 0; }
  .notice { color: rgb(var(--theme-text-tertiary)); font-size: 12px; line-height: 1.6; margin: 12px 0 24px; }
  details { font-size: 12px; color: rgb(var(--theme-text-secondary)); margin: 24px 0; overflow-wrap: anywhere; }
  summary { cursor: pointer; padding-block: 8px; }
</style>
