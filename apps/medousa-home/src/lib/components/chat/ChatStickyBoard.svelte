<script lang="ts">
  import { tick } from 'svelte';
  import { readChatStickies, mutateChatStickies, type StickyBoard, type StickyNote, type StickyMutationRequest, type StickyOperation } from '$lib/daemon/chatStickies';

  let { sessionId, workshopScopeId, profileId, enabled = true }: { sessionId: string; workshopScopeId?: string; profileId?: string | null; enabled?: boolean } = $props();
  let board = $state<StickyBoard | null>(null);
  let open = $state(false);
  let busy = $state(false);
  let error = $state('');
  let editing = $state<StickyNote | null>(null);
  let editRevision = $state<number | null>(null);
  let draft = $state('');
  let pinned = $state(true);
  let exact = $state(false);
  let showResolved = $state(false);
  let removing = $state<string | null>(null);
  let pending = $state<StickyMutationRequest | null>(null);
  let dialog = $state<HTMLDialogElement>();
  let generation = 0;
  const active = $derived(board?.notes.filter(note => note.status === 'active') ?? []);
  const notes = $derived(board?.notes.filter(note => showResolved || note.status === 'active') ?? []);

  $effect(() => {
    const chat = sessionId;
    const scope = workshopScopeId;
    const available = enabled;
    // Profile changes invalidate pending reads even if the selected chat is unchanged.
    void profileId;
    generation++;
    board = null; error = ''; busy = false; pending = null; editing = null; editRevision = null; draft = ''; removing = null; open = false;
    dialog?.close();
    if (available && chat) void load(chat, scope, generation);
  });
  $effect(() => {
    if (!open || !enabled) return;
    // Refresh only while inspected. Never overwrite an unsaved edit or unknown mutation.
    const timer = setInterval(() => { if (!busy && !pending && !editing && !draft) void refresh(); }, 5000);
    return () => clearInterval(timer);
  });
  async function load(chat: string, scope: string | undefined, version: number) {
    busy = true;
    try {
      const result = await readChatStickies(chat, scope);
      if (version === generation) { board = result; error = ''; }
    } catch (cause) {
      if (version === generation) error = String(cause);
    } finally { if (version === generation) busy = false; }
  }
  async function refresh() {
    if (busy) return;
    await load(sessionId, workshopScopeId, generation);
  }
  async function inspect() {
    open = true;
    await tick();
    if (open && dialog && !dialog.open) dialog.showModal();
    await refresh();
  }
  function edit(note: StickyNote) { editing = note; editRevision = board?.revision ?? null; draft = note.text; pinned = note.pinned; exact = note.exact; removing = null; }
  function cancelEdit() { editing = null; editRevision = null; draft = ''; pinned = true; exact = false; }
  async function submit(operation: StickyOperation, expectedRevision = board?.revision) {
    if (!board || busy || pending || expectedRevision === undefined) return;
    pending = { expected_revision: expectedRevision, request_id: crypto.randomUUID(), operation };
    await retry();
  }
  async function retry() {
    if (!pending || busy) return;
    const request = pending;
    const version = generation;
    const chat = sessionId;
    const scope = workshopScopeId;
    busy = true; error = '';
    try {
      const result = await mutateChatStickies(chat, request, scope);
      if (version === generation) { board = result; pending = null; removing = null; cancelEdit(); }
    } catch (cause) {
      if (version === generation) error = `${String(cause)}. The change is not confirmed. Retry reuses the same request; reload to inspect the latest board.`;
    } finally { if (version === generation) busy = false; }
  }
  async function reloadAfterFailure() {
    if (busy) return;
    await refresh();
    if (!error) pending = null;
  }
  async function save() {
    if (!draft.trim()) return;
    await submit({ action: 'put', note: { id: editing?.id ?? crypto.randomUUID(), text: draft, kind: editing?.kind ?? 'detail', pinned, exact, source_refs: editing?.source_refs ?? [], status: editing?.status ?? 'active' } }, editing ? editRevision ?? undefined : board?.revision);
  }
</script>

{#if enabled && sessionId}
  <div class="sticky-control">
    <button type="button" class="sticky-trigger" onclick={() => void inspect()} aria-haspopup="dialog">Sticky notes{board ? ` · ${active.length}` : ''}</button>
  </div>
  <dialog bind:this={dialog} aria-label="Chat sticky notes" onclose={() => open = false} onclick={(event) => { if (event.target === dialog) dialog.close(); }}>
    <header><div><strong>Chat sticky notes</strong><p>Working memory for this chat · Not permanent memory</p></div><button type="button" aria-label="Close sticky notes" onclick={() => dialog?.close()}>✕</button></header>
    <p class="hint">Medousa maintains these details across turns and modes. You can edit, resolve, or remove them. Pinned and exact details are protected from rollup.</p>
    <div class="toolbar"><button type="button" disabled={busy} onclick={() => void (pending ? reloadAfterFailure() : refresh())}>Reload latest</button><label><input type="checkbox" bind:checked={showResolved} /> Show resolved</label><span>{board ? `Revision ${board.revision}` : 'Not loaded'}</span></div>
    {#if error}<p role="alert">{error}</p>{/if}
    {#if pending && error}<button type="button" disabled={busy} onclick={() => void retry()}>Retry same change</button>{/if}
    {#if busy}<p role="status">Updating sticky notes…</p>{/if}
    {#if board}
      <ul>
        {#each notes as note (note.id)}
          <li>
            <div class="badges">{note.kind} · {note.status}{note.pinned ? ' · Pinned' : ''}{note.exact ? ' · Exact' : ''}</div>
            <p class="note-text">{note.text}</p>
            {#if note.source_refs.length}<details><summary>Sources</summary>{#each note.source_refs as source}<p class="source">{source}</p>{/each}</details>{/if}
            <div class="note-actions">
              <button type="button" disabled={busy || !!pending} onclick={() => edit(note)}>Edit</button>
              <button type="button" disabled={busy || !!pending} onclick={() => void submit({ action: note.status === 'active' ? 'resolve' : 'reopen', id: note.id })}>{note.status === 'active' ? 'Resolve' : 'Reopen'}</button>
              <button type="button" disabled={busy || !!pending} onclick={() => removing = note.id}>Remove</button>
            </div>
            {#if removing === note.id}<div class="confirm">Remove this note from this chat? <button type="button" disabled={busy || !!pending} onclick={() => void submit({ action: 'remove', id: note.id })}>Confirm removal</button><button type="button" onclick={() => removing = null}>Keep note</button></div>{/if}
          </li>
        {:else}<li class="hint">No {showResolved ? '' : 'active '}sticky notes yet.</li>{/each}
      </ul>
      <form onsubmit={(event) => { event.preventDefault(); void save(); }}>
        <label for="sticky-text">{editing ? 'Edit sticky note' : 'Add a detail to keep hot'}</label>
        {#if editing && editRevision !== board.revision}<p role="status">The board changed since this edit began. Cancel the edit and reopen the latest note before saving.</p>{/if}
        <textarea id="sticky-text" bind:value={draft} rows="3" disabled={busy || !!pending} placeholder="A consequential correction, exact detail, or follow-up…"></textarea>
        <div class="toolbar"><label><input type="checkbox" bind:checked={pinned} disabled={busy || !!pending} /> Pinned</label><label><input type="checkbox" bind:checked={exact} disabled={busy || !!pending} /> Preserve exact text</label></div>
        <button type="submit" disabled={busy || !!pending || !draft.trim()}>{editing ? 'Save changes' : 'Add sticky note'}</button>
        {#if editing}<button type="button" onclick={cancelEdit} disabled={busy || !!pending}>Cancel edit</button>{/if}
      </form>
    {/if}
  </dialog>
{/if}

<style>
  .sticky-control { width: min(100% - 32px, 700px); margin: 0 auto 4px; }
  button { color: rgb(var(--theme-text-primary)); background: transparent; border: 1px solid rgb(var(--theme-border) / .4); border-radius: 8px; padding: 5px 9px; cursor: pointer; font-size: 12px; }
  button:disabled { opacity: .5; cursor: default; }
  .sticky-trigger { border: 0; padding: 4px 0; color: rgb(var(--theme-text-secondary)); }
  dialog { box-sizing: border-box; width: min(640px, calc(100vw - 24px)); max-height: min(80dvh, 850px); overflow: auto; margin: auto; padding: 20px; border: 1px solid rgb(var(--theme-border)); border-radius: 16px; color: rgb(var(--theme-text-primary)); background: rgb(var(--theme-card)); box-shadow: 0 20px 70px #0005; }
  dialog::backdrop { background: #0006; }
  header, .toolbar, .note-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
  header { justify-content: space-between; }
  header p, .hint, .badges, .source, .toolbar { font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  .toolbar { margin: 12px 0; }
  .toolbar span { margin-left: auto; }
  label { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; }
  ul { list-style: none; padding: 0; }
  li { border-top: 1px solid rgb(var(--theme-border) / .5); padding: 14px 0; }
  .note-text, .source { white-space: pre-wrap; overflow-wrap: anywhere; }
  .confirm, [role='alert'] { font-size: 12px; color: rgb(var(--theme-warning)); margin: 10px 0; }
  textarea { width: 100%; box-sizing: border-box; margin: 8px 0; padding: 10px; border: 1px solid rgb(var(--theme-border)); border-radius: 8px; background: rgb(var(--theme-card)); color: inherit; resize: vertical; }
</style>
