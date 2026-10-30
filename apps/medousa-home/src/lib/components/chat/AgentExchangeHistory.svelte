<script lang="ts">
  import { untrack } from 'svelte';
  import { getSessionHistory } from '$lib/daemon';
  import { mapTurns, TRANSCRIPT_PAGE_SIZE } from '$lib/chat/sessionController';
  import type { ChatMessage } from '$lib/types/chat';
  import ExchangeMessage from './ExchangeMessage.svelte';
  let { sessionId, authorityId, participant }: { sessionId: string; authorityId: string; participant: string } = $props();
  let messages = $state<ChatMessage[]>([]);
  let cursors = $state<(string | undefined)[]>([undefined]);
  let nextCursor = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let generation = 0;
  async function load(cursor?: string, direction: 'older' | 'newer' | 'latest' | 'refresh' = 'refresh') {
    if (busy) return;
    const token = generation;
    const id = sessionId, authority = authorityId;
    busy = true;
    error = null;
    try {
      const result = await getSessionHistory(id, { limit: TRANSCRIPT_PAGE_SIZE, ...(cursor ? { cursor } : {}) });
      if (token !== generation) return;
      if (result.session_id !== id || result.authority_id !== authority) throw new Error('Exchange authority changed. Return to our chat and reopen it.');
      messages = mapTurns(result.turns, { sessionId: id, authorityId: authority }).filter(message => message.role === 'user' || message.role === 'assistant');
      nextCursor = result.next_cursor && result.next_cursor !== cursor ? result.next_cursor : null;
      if (direction === 'older') cursors = [...cursors, cursor];
      else if (direction === 'newer') cursors = cursors.slice(0, -1);
      else if (direction === 'latest') cursors = [undefined];
    } catch (cause) { if (token === generation) error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (token === generation) busy = false; }
  }
  $effect(() => {
    sessionId; authorityId;
    generation++; busy = false; messages = []; cursors = [undefined]; nextCursor = null;
    untrack(() => void load(undefined, 'latest'));
    return () => { generation++; };
  });
  $effect(() => {
    if (cursors.length !== 1) return;
    const timer = setInterval(() => { if (!busy && !document.hidden) void load(); }, 5000);
    return () => clearInterval(timer);
  });
</script>
<div class="history-toolbar">
  {#if nextCursor}<button type="button" disabled={busy} onclick={() => void load(nextCursor!, 'older')}>Earlier messages</button>{/if}
  {#if cursors.length > 1}<button type="button" disabled={busy} onclick={() => void load(cursors.at(-2), 'newer')}>Later messages</button>{/if}
  <button type="button" disabled={busy} onclick={() => void load(undefined, 'latest')}>Refresh exchange</button>
</div>
{#if error}<p role="alert">{error}</p>{/if}
{#each messages as message (message.id)}
  {#if message.content.trim()}<ExchangeMessage label={message.role === 'user' ? 'Medousa · Instructions' : participant} text={message.content} sender={message.role === 'user'} />{/if}
{:else}
  <p class="notice" role="status">{busy ? 'Loading recorded messages…' : 'No conversation messages are available. The assignment and any returned result are shown below.'}</p>
{/each}
<style>
  .history-toolbar { display: flex; flex-wrap: wrap; gap: 12px; justify-content: flex-end; margin: 12px 0 24px; }
  button { border: 0; border-radius: 20px; padding: 8px 12px; color: rgb(var(--theme-text-secondary)); background: rgb(var(--theme-card-hover) / .5); font-size: 12px; cursor: pointer; }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 2px; }
  button:disabled { opacity: .5; cursor: default; }
  .notice { color: rgb(var(--theme-text-secondary)); font-size: 12px; margin-block: 16px; }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
