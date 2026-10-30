<script lang="ts">
  import { untrack } from 'svelte';
  import { getSessionHistory } from '$lib/daemon';
  import { mapTurns, TRANSCRIPT_PAGE_SIZE } from '$lib/chat/sessionController';
  import type { ChatMessage } from '$lib/types/chat';
  import AgentRecordedMessage from './AgentRecordedMessage.svelte';
  import ExchangeMessage from './ExchangeMessage.svelte';
  let { sessionId, authorityId, participant, fallbackResult = '', fallbackLabel = 'Returned result' }: { sessionId: string; authorityId: string; participant: string; fallbackResult?: string; fallbackLabel?: string } = $props();
  let messages = $state<ChatMessage[]>([]);
  let nextCursor = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let generation = 0;
  let loadedOlder = false;
  let visitedCursors = new Set<string>();
  const normalize = (text: string) => text.trim().replace(/\s+/g, ' ');
  const resultRecorded = $derived.by(() => {
    const result = normalize(fallbackResult);
    return Boolean(result) && messages.some(message => {
      if (message.role !== 'assistant') return false;
      const recordedText = (message.segments ?? []).flatMap(segment => segment.kind === 'text' ? [segment.markdown] : []).join('\n\n');
      return [message.content, recordedText].some(text => normalize(text).includes(result));
    });
  });

  async function load(cursor?: string) {
    if (busy) return;
    const token = generation;
    const id = sessionId, authority = authorityId;
    busy = true;
    error = null;
    try {
      const result = await getSessionHistory(id, { limit: TRANSCRIPT_PAGE_SIZE, ...(cursor ? { cursor } : {}) });
      if (token !== generation) return;
      if (result.session_id !== id || result.authority_id !== authority) throw new Error('Exchange authority changed. Return to our chat and reopen it.');
      const incoming = mapTurns(result.turns, { sessionId: id, authorityId: authority }).filter(message => message.role === 'user' || message.role === 'assistant');
      const updated = new Map(incoming.map(message => [message.id, message]));
      const retained = messages.map(message => updated.get(message.id) ?? message);
      const existing = new Set(messages.map(message => message.id));
      const added = incoming.filter(message => !existing.has(message.id));
      messages = (cursor ? [...added, ...retained] : [...retained, ...added]).sort((left, right) => {
        const a = left.transcript?.entrySeq, b = right.transcript?.entrySeq;
        return a != null && b != null ? a - b : 0;
      });
      if (cursor) { loadedOlder = true; visitedCursors.add(cursor); }
      if (cursor) nextCursor = result.next_cursor && !visitedCursors.has(result.next_cursor) ? result.next_cursor : null;
      else if (result.next_cursor && !visitedCursors.has(result.next_cursor)) nextCursor = result.next_cursor;
      else if (!loadedOlder) nextCursor = null;
    } catch (cause) { if (token === generation) error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (token === generation) busy = false; }
  }
  $effect(() => {
    sessionId; authorityId;
    generation++; busy = false; messages = []; nextCursor = null;
    loadedOlder = false; visitedCursors = new Set();
    untrack(() => void load());
    return () => { generation++; };
  });
  $effect(() => {
    const timer = setInterval(() => { if (!busy && !document.hidden) void load(); }, 5000);
    return () => clearInterval(timer);
  });
</script>
<div class="history-toolbar">
  {#if nextCursor}<button type="button" disabled={busy} onclick={() => void load(nextCursor!)}>Earlier messages</button>{/if}
  <button type="button" disabled={busy} onclick={() => void load()}>Refresh exchange</button>
</div>
{#if error}<p role="alert">{error}</p>{/if}
{#each messages as message (message.id)}
  <AgentRecordedMessage {message} {participant} {sessionId} />
{:else}
  <p class="notice" role="status">{busy ? 'Loading recorded messages…' : 'No conversation messages are available. The assignment and any returned result are shown below.'}</p>
{/each}
{#if fallbackResult.trim() && !resultRecorded}<ExchangeMessage label={`${participant} · ${fallbackLabel}`} text={fallbackResult} />{/if}
<style>
  .history-toolbar { display: flex; flex-wrap: wrap; gap: 12px; justify-content: flex-end; margin: 12px 0 24px; }
  button { border: 0; border-radius: 20px; padding: 8px 12px; color: rgb(var(--theme-text-secondary)); background: rgb(var(--theme-card-hover) / .5); font-size: 12px; cursor: pointer; }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 2px; }
  button:disabled { opacity: .5; cursor: default; }
  .notice { color: rgb(var(--theme-text-secondary)); font-size: 12px; margin-block: 16px; }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
