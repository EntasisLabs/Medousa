<script lang="ts">
  import { tick } from 'svelte';
  import { StickyNote as StickyNoteIcon, ChevronDown, X } from '@lucide/svelte';
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
  let trigger = $state<HTMLButtonElement>();
  let adding = $state(false);
  let menuId = $state<string | null>(null);
  let position = $state({ left: 12, top: 12, maxHeight: 500 });
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
    board = null; error = ''; busy = false; pending = null; editing = null; editRevision = null; draft = ''; removing = null; adding = false; menuId = null; open = false;
    dialog?.close();
    if (available && chat) void load(chat, scope, generation);
  });
  $effect(() => {
    if (!open || !enabled) return;
    // Refresh only while inspected. Never overwrite an unsaved edit or unknown mutation.
    const timer = setInterval(() => { if (!busy && !pending && !editing && !adding && !draft) void refresh(); }, 5000);
    return () => clearInterval(timer);
  });
  $effect(() => {
    if (!open || !dialog || typeof ResizeObserver === 'undefined') return;
    // Keep the notebook attached to its trigger as editors and menus change height.
    const observer = new ResizeObserver(place);
    observer.observe(dialog);
    return () => observer.disconnect();
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
  function place() {
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const height = Math.max(120, Math.min(560, window.innerHeight - 24));
    const above = rect.top - 10;
    const below = window.innerHeight - rect.bottom - 10;
    const useAbove = above >= Math.min(360, height) || above > below;
    const maxHeight = Math.max(120, Math.min(height, useAbove ? above - 12 : below - 12));
    const width = Math.min(400, window.innerWidth - 24);
    position = {
      left: Math.max(12, Math.min(rect.right - width, window.innerWidth - width - 12)),
      top: useAbove ? Math.max(12, rect.top - 10 - Math.min(dialog?.offsetHeight || maxHeight, maxHeight)) : rect.bottom + 10,
      maxHeight,
    };
  }
  function dismiss(returnFocus = true) {
    open = false; menuId = null; removing = null;
    dialog?.close();
    if (returnFocus) trigger?.focus();
  }
  function outside(event: PointerEvent) {
    if (open && event.target instanceof Node && !dialog?.contains(event.target) && !trigger?.contains(event.target)) dismiss(false);
  }
  async function inspect() {
    if (open) { dismiss(); return; }
    open = true;
    await tick();
    if (open && dialog && !dialog.open) { dialog.show(); place(); dialog.focus(); }
    await refresh();
    if (open) { await tick(); place(); }
  }
  async function focusEditor() { await tick(); dialog?.querySelector('textarea')?.focus(); }
  function edit(note: StickyNote) {
    editing = note; adding = false; editRevision = board?.revision ?? null; draft = note.text;
    pinned = note.pinned; exact = note.exact; removing = null; menuId = null; void focusEditor();
  }
  function add() { cancelEdit(); adding = true; void focusEditor(); }
  function cancelEdit() { editing = null; adding = false; editRevision = null; draft = ''; pinned = true; exact = false; }
  async function toggleExact(note: StickyNote) {
    menuId = null;
    await submit({ action: 'put', note: { ...note, exact: !note.exact } });
  }
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

<svelte:window onpointerdown={outside} onkeydown={(event) => {
  if (open && event.key === 'Escape') { event.preventDefault(); dismiss(); }
}} onresize={place} onscroll={() => { if (open) place(); }} />

{#snippet editor()}
  <form class="editor" onsubmit={(event) => { event.preventDefault(); void save(); }}>
    <label for="sticky-text">{editing ? 'Edit sticky note' : 'Add a note'}</label>
    {#if editing && editRevision !== board?.revision}<p role="status">The board changed since this edit began. Cancel the edit and reopen the latest note before saving.</p>{/if}
    <textarea id="sticky-text" bind:value={draft} rows="3" disabled={busy || !!pending} placeholder="Something worth keeping in mind…"></textarea>
    <div class="editor-options"><label><input type="checkbox" bind:checked={pinned} disabled={busy || !!pending} /> Pinned</label><label><input type="checkbox" bind:checked={exact} disabled={busy || !!pending} /> Preserve exact text</label></div>
    <div class="editor-actions"><button class="save" type="submit" disabled={busy || !!pending || !draft.trim()}>{editing ? 'Save changes' : 'Add sticky note'}</button><button type="button" onclick={cancelEdit} disabled={busy || !!pending}>Cancel</button></div>
  </form>
{/snippet}

{#if enabled && sessionId}
  <div class="sticky-control">
    <button bind:this={trigger} type="button" class="sticky-trigger" onclick={() => void inspect()} aria-haspopup="dialog" aria-expanded={open} aria-label={`Sticky notes${board ? ` · ${active.length}` : ''}`}><StickyNoteIcon size={16} strokeWidth={1.75} aria-hidden="true" /><span><span class="sticky-prefix">{'Sticky '}</span>notes{board ? ` · ${active.length}` : ''}</span><ChevronDown size={12} aria-hidden="true" class={open ? 'chevron expanded' : 'chevron'} /></button>
  </div>
  <dialog bind:this={dialog} aria-label="Kept in mind" tabindex="-1" onclose={() => open = false}
    style:left={`${position.left}px`} style:top={`${position.top}px`} style:max-height={`${position.maxHeight}px`}>
    <header><div><strong>Kept in mind</strong><p>This chat</p></div><button type="button" class="icon-button" aria-label="Close sticky notes" onclick={() => dismiss()}><X size={16} aria-hidden="true" /></button></header>
    <div class="toolbar"><button type="button" class:chosen={showResolved} aria-pressed={showResolved} onclick={() => { showResolved = !showResolved; }}>{showResolved ? 'Hide resolved' : 'Show resolved'}</button><button type="button" disabled={busy} onclick={() => void (pending ? reloadAfterFailure() : refresh())}>Reload latest</button></div>
    {#if error}<p class="notice" role="alert">{error}</p>{/if}
    {#if pending && error}<button type="button" disabled={busy} onclick={() => void retry()}>Retry same change</button>{/if}
    {#if busy}<p class="notice" role="status">Updating sticky notes…</p>{/if}
    {#if board}
      <ul>
        {#each notes as note (note.id)}
          <li class:resolved={note.status === 'resolved'}>
            {#if editing?.id === note.id}
              {@render editor()}
            {:else}
              <div class="note-row">
                <button class="note-text" type="button" disabled={busy || !!pending || adding || !!editing} aria-label={`Edit note: ${note.text}`} onclick={() => edit(note)}>{note.text}</button>
                <button class="icon-button overflow" type="button" aria-label={`Actions for note: ${note.text}`} aria-expanded={menuId === note.id} disabled={busy || !!pending || adding || !!editing} onclick={() => { menuId = menuId === note.id ? null : note.id; removing = null; }}>⋯</button>
              </div>
              {#if note.pinned || note.exact || note.status === 'resolved'}<div class="badges">{[note.pinned ? 'Pinned' : '', note.exact ? 'Exact' : '', note.status === 'resolved' ? 'Resolved' : ''].filter(Boolean).join(' · ')}</div>{/if}
              {#if menuId === note.id}
                <div class="note-menu" aria-label="Note actions">
                  <button type="button" disabled={busy || !!pending} onclick={() => { menuId = null; void submit({ action: 'pin', id: note.id, pinned: !note.pinned }); }}>{note.pinned ? 'Unpin' : 'Pin'}</button>
                  <button type="button" disabled={busy || !!pending} onclick={() => void toggleExact(note)}>{note.exact ? 'Allow summarizing' : 'Preserve exact text'}</button>
                  <button type="button" disabled={busy || !!pending} onclick={() => { menuId = null; void submit({ action: note.status === 'active' ? 'resolve' : 'reopen', id: note.id }); }}>{note.status === 'active' ? 'Resolve' : 'Reopen'}</button>
                  <button type="button" disabled={busy || !!pending} onclick={() => { removing = note.id; menuId = null; }}>Remove</button>
                  {#if note.source_refs.length}<details><summary>Sources</summary>{#each note.source_refs as source}<p class="source">{source}</p>{/each}</details>{/if}
                </div>
              {/if}
              {#if removing === note.id}<div class="confirm">Remove this note from this chat?<div><button type="button" disabled={busy || !!pending} onclick={() => void submit({ action: 'remove', id: note.id })}>Confirm removal</button><button type="button" onclick={() => removing = null}>Keep note</button></div></div>{/if}
            {/if}
          </li>
        {:else}<li class="empty">Nothing {showResolved ? '' : 'active '}kept here yet.<span>Useful details from this chat will live here.</span></li>{/each}
      </ul>
      {#if editing && !notes.some(note => note.id === editing?.id)}{@render editor()}{/if}
      {#if adding}{@render editor()}{:else}<button type="button" class="add-note" disabled={busy || !!pending || !!editing} onclick={add}>+ Add a note</button>{/if}
      <footer>Working context, not permanent memory</footer>
    {/if}
  </dialog>
{/if}

<style>
  .sticky-control { flex: 0 0 auto; margin-left: auto; }
  button { color: inherit; background: transparent; border: 0; border-radius: 7px; padding: 6px 9px; cursor: pointer; font: inherit; font-size: 12px; }
  button:hover:not(:disabled) { background: rgb(var(--theme-text-primary) / .06); }
  button:focus-visible, summary:focus-visible, textarea:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 2px; }
  button:disabled { opacity: .5; cursor: default; }
  .sticky-trigger { display: inline-flex; align-items: center; gap: 7px; min-height: 32px; padding: 6px 8px; margin-right: -8px; border-radius: 9px; color: rgb(var(--theme-text-secondary)); font-size: 12px; line-height: 20px; white-space: nowrap; transition: background-color 120ms ease, color 120ms ease; }
  @media (max-width: 600px) { .sticky-trigger :global(.chevron), .sticky-prefix { display: none; } }
  .sticky-trigger[aria-expanded='true'] { background: rgb(var(--theme-card-hover)); color: rgb(var(--theme-text-primary)); }
  .sticky-trigger :global(.chevron) { transition: transform 120ms ease; }
  .sticky-trigger :global(.chevron.expanded) { transform: rotate(180deg); }
  dialog { position: fixed; box-sizing: border-box; width: min(400px, calc(100vw - 24px)); overflow: auto; margin: 0; padding: 18px; border: 1px solid rgb(var(--theme-border) / .6); border-radius: 16px; color: rgb(var(--theme-text-primary)); background: rgb(var(--theme-card)); box-shadow: 0 12px 40px #0003; z-index: 50; }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  header strong { font-size: 15px; font-weight: 600; }
  header p { margin: 4px 0 0; font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  .icon-button { display: inline-flex; align-items: center; justify-content: center; flex: 0 0 auto; width: 28px; height: 28px; padding: 2px; }
  .toolbar { display: flex; justify-content: space-between; margin: 14px -6px 4px; color: rgb(var(--theme-text-secondary)); }
  .toolbar button { font-size: 11px; padding: 5px 6px; }
  .toolbar .chosen { color: rgb(var(--theme-text-primary)); background: rgb(var(--theme-text-primary) / .06); }
  ul { list-style: none; padding: 0; margin: 0; }
  li { position: relative; padding: 12px 0; }
  .note-row { display: flex; align-items: flex-start; gap: 6px; }
  .note-text { flex: 1; min-width: 0; padding: 2px 0; text-align: left; font-size: 14px; line-height: 1.65; white-space: pre-wrap; overflow-wrap: anywhere; }
  .note-text:hover:not(:disabled) { background: transparent; color: rgb(var(--theme-link)); }
  .overflow, .badges, .source, .empty, footer { color: rgb(var(--theme-text-secondary)); }
  .badges { margin-top: 5px; font-size: 10px; letter-spacing: .02em; }
  .resolved .note-text { color: rgb(var(--theme-text-secondary)); }
  .note-menu { display: flex; flex-direction: column; align-items: stretch; gap: 2px; margin: 8px 0 0 auto; padding: 6px; width: 190px; border: 1px solid rgb(var(--theme-border) / .5); border-radius: 10px; background: rgb(var(--theme-card)); box-shadow: 0 4px 16px #0001; }
  .note-menu button { text-align: left; }
  details { padding: 6px 9px; font-size: 12px; }
  summary { cursor: pointer; }
  .source { white-space: pre-wrap; overflow-wrap: anywhere; }
  .confirm, .notice { font-size: 12px; line-height: 1.5; color: rgb(var(--theme-text-secondary)); margin: 10px 0; }
  .confirm { padding: 10px; background: rgb(var(--theme-text-primary) / .04); border-radius: 8px; }
  .confirm div { margin-top: 6px; }
  [role='alert'] { color: rgb(var(--theme-warning)); }
  .empty { font-size: 13px; line-height: 1.7; padding: 20px 0; }
  .empty span { display: block; font-size: 12px; }
  .editor { margin: 4px 0 8px; }
  label { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; }
  textarea { width: 100%; box-sizing: border-box; margin: 8px 0; padding: 10px; border: 1px solid rgb(var(--theme-border) / .7); border-radius: 8px; background: rgb(var(--theme-card)); color: inherit; font: inherit; font-size: 13px; line-height: 1.6; resize: vertical; }
  .editor-options, .editor-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; margin-bottom: 10px; }
  .save { background: rgb(var(--theme-text-primary) / .08); }
  .add-note { color: rgb(var(--theme-text-secondary)); margin: 6px -8px 0; }
  footer { border-top: 1px solid rgb(var(--theme-border) / .35); padding-top: 12px; margin-top: 14px; font-size: 10px; }
  @media (pointer: coarse) { button, summary { min-height: 44px; } .icon-button { width: 44px; } }
</style>
