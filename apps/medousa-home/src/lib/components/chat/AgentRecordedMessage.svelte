<script lang="ts">
  import type { ChatMessage } from '$lib/types/chat';
  import ExchangeMessage from './ExchangeMessage.svelte';
  import ToolRunChips from './ToolRunChips.svelte';
  let { message, participant, sessionId }: { message: ChatMessage; participant: string; sessionId: string } = $props();
  const label = $derived(message.role === 'user' ? 'Medousa · Instructions' : participant);
  const textSegments = $derived((message.segments ?? []).flatMap(segment => segment.kind === 'text' || segment.kind === 'progress' ? [segment.markdown] : []));
  const representedRuns = $derived(new Set((message.segments ?? []).flatMap(segment => segment.kind === 'tool_group' ? segment.runs.map(run => run.runId) : [])));
  const extraRuns = $derived((message.toolRuns ?? []).filter(run => !representedRuns.has(run.runId)));
  // Adjacent tool rounds share a compact entry, but never move across text.
  const segments = $derived.by(() => {
    const grouped: NonNullable<ChatMessage['segments']> = [];
    for (const segment of message.segments ?? []) {
      const previous = grouped.at(-1);
      if (segment.kind === 'tool_group' && previous?.kind === 'tool_group') {
        previous.runs.push(...segment.runs);
      } else grouped.push(segment.kind === 'tool_group' ? { ...segment, runs: [...segment.runs] } : segment);
    }
    return grouped;
  });
  const normalize = (text: string) => text.trim().replace(/\s+/g, ' ');
  const contentRecorded = $derived(Boolean(message.content.trim()) && normalize(textSegments.join('\n\n')).includes(normalize(message.content)));
</script>
<article aria-label={`${label} · Recorded message`}>
  {#each segments as segment}
    {#if segment.kind === 'text' || segment.kind === 'progress'}
      <ExchangeMessage {label} text={segment.markdown} sender={message.role === 'user'} />
    {:else if segment.kind === 'tool_group'}
      <ToolRunChips runs={segment.runs} {sessionId} turnIndex={message.turnIndex} inspectorPresentation="inline" />
    {:else if segment.kind === 'handoff'}
      <ExchangeMessage label={`${label} · Handoff`} text={segment.text} />
    {:else if segment.kind === 'artifact'}
      <p>Recorded artifact: {segment.artifact.label} ({segment.artifact.mime})</p>
    {/if}
  {/each}
  {#if message.content.trim() && !contentRecorded}
    <ExchangeMessage {label} text={message.content} timestamp={message.createdAt} sender={message.role === 'user'} />
  {/if}
  {#if extraRuns.length}<ToolRunChips runs={extraRuns} {sessionId} turnIndex={message.turnIndex} inspectorPresentation="inline" />{/if}
  {#if message.errorLine}<p role="alert">{message.errorLine}</p>{/if}
  {#if message.errorDetail}<details><summary>Error details</summary><pre>{message.errorDetail}</pre></details>{/if}
</article>
<style>
  article { min-width: 0; margin-block: 20px; }
  p, details { color: rgb(var(--theme-text-secondary)); font-size: 12px; overflow-wrap: anywhere; }
  pre { white-space: pre-wrap; }
</style>
