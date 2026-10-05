<script lang="ts">
  import { onMount } from "svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { admitRuntimeBotTurn, cancelRuntimeBotTurn, loadRuntimeBotJobs, refreshRuntimeBotTurn, runtimeBotJob, runtimeBotJobPending } from "$lib/chat/runtimeBotTurns.svelte";
  let { sessionId }: { sessionId: string } = $props();
  const job = $derived(runtimeBotJob(sessionId)); const pending = $derived(runtimeBotJobPending(sessionId));
  $effect(() => {chat.workshopScopeId;loadRuntimeBotJobs();});
  onMount(() => {
    const refresh = () => { if (!connection.offline && document.visibilityState === "visible") void refreshRuntimeBotTurn(sessionId); };
    refresh(); const timer = window.setInterval(refresh,2000); return () => window.clearInterval(timer);
  });
  const labels: Record<string,string> = {submitting:"Sending to your Bot…",submission_uncertain:"Submission needs attention",transport_pending:"Waiting for the Bot’s workshop…",destination_admitted:"Accepted by the Bot’s workshop",running:"Your Bot is working…",failed:"Bot request failed",cancelled:"Bot request cancelled"};
  async function retry() {try {if (await admitRuntimeBotTurn(sessionId)) chat.clearStreamError(sessionId);} catch {/* Retain the durable retry ID. */}}
  async function cancel() {try {await cancelRuntimeBotTurn(sessionId);} catch(cause){chat.setError(cause instanceof Error ? cause.message : String(cause));}}
</script>
{#if job && (pending || job.error)}
  <div class="status" role="status"><span>{labels[job.status] ?? job.status}{#if job.error}<small>{job.error}</small>{/if}</span>
    {#if job.status === "submission_uncertain"}<button type="button" disabled={connection.offline} onclick={() => void retry()}>Retry submission</button>{:else if pending && job.jobId}<button type="button" disabled={connection.offline} onclick={() => void cancel()}>Cancel</button>{/if}
  </div>
{/if}
<style>.status{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:8px 12px;font-size:12px;color:rgb(var(--theme-text-secondary));}small{display:block;color:#f3a1ad;margin-top:4px;}button{flex-shrink:0;padding:6px 8px;border-radius:8px;background:rgb(var(--theme-border) / .2);}</style>
