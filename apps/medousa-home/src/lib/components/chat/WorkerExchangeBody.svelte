<script lang="ts">
  import { untrack } from 'svelte';
  import { workerTranscripts } from '$lib/work/workerTranscripts.svelte';
  import ExchangeMessage from './ExchangeMessage.svelte';
  let { workId }: { workId: string } = $props();
  let failed = $state(false);
  const task = $derived(workerTranscripts.transcriptFor(workId));
  const answer = $derived(task?.resultText?.trim() || task?.output?.trim() || '');
  $effect(() => {
    const id = workId;
    let current = true;
    failed = false;
    untrack(() => { void workerTranscripts.refresh(id).then(result => { if (current) failed = !result; }); });
    return () => { current = false; };
  });
</script>
{#if task}
  <h1>{task.title}</h1>
  <p class="status">{task.streaming ? 'Working' : task.terminal ? 'Session ended' : task.statusLine || 'Status unavailable'}</p>
  <p class="notice">Recorded assignment summary and latest output · This worker does not expose a separate message history.</p>
  <ExchangeMessage label="Medousa · Assignment summary" text={task.title} sender />
  {#if answer}<ExchangeMessage label={task.streaming ? 'Agent · Latest output' : 'Agent · Result'} text={answer} />
  {:else}<p role="status">{task.streaming ? 'The agent is working. Its output will appear here.' : 'No written result was recorded.'}</p>{/if}
  {#if task.error}<p role="alert">{task.error}</p>{/if}
  <details><summary>Technical details</summary><p>Work: {workId}</p>{#if task.model}<p>Model: {task.model}</p>{/if}<p>{task.toolRuns.length} recorded tool runs</p></details>
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
