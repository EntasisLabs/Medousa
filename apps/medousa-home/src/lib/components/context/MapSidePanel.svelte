<script lang="ts">
  import { tick, untrack } from "svelte";
  import { Search } from "@lucide/svelte";
  import { contextNotes } from "$lib/context/contextNotes.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { contextShell } from "$lib/context/contextShell.svelte";
  import { contextThreads } from "$lib/context/contextThreads.svelte";
  import { contextBrowseEntries } from "$lib/utils/contextBrowse";
  import { formatContextWhen } from "$lib/utils/contextHuman";

  let { onPick }: { onPick?: () => void } = $props();
  let shown = $state(50);
  let listEl = $state<HTMLDivElement>();
  const sessionLabels = $derived(Object.fromEntries(chat.sessions.map((session) => [session.session_id, session.display_name?.trim() || session.session_id])));
  const entries = $derived(contextBrowseEntries(contextThreads.nodes, sessionLabels, contextShell.search));
  const selectedIndex = $derived(entries.findIndex((entry) => entry.id === contextShell.selectedMapNodeId));
  const visibleCount = $derived(Math.max(shown, selectedIndex + 1));

  $effect(() => { contextShell.search; shown = 50; });
  $effect(() => {
    chat.workshopScopeId;
    void contextThreads.refresh({ limit: 200 });
    void chat.refreshSessions();
    void contextNotes.refresh();
  });

  $effect(() => {
    const selected = contextShell.selectedMapNodeId;
    const list = listEl;
    if (!selected || !list) return;
    void tick().then(() => {
      if (contextShell.selectedMapNodeId !== selected) return;
      const row = [...list.querySelectorAll<HTMLButtonElement>("button[data-memory-id]")].find((button) => button.dataset.memoryId === selected);
      row?.scrollIntoView?.({ block: "nearest" });
    });
  });

  function selectMemory(id: string) {
    contextShell.selectMapNode(id);
    untrack(() => void contextThreads.loadDetail(id.slice("thread:".length)));
    onPick?.();
  }
</script>

<section class="context-memory-list flex h-full min-h-0 flex-col" aria-label="Saved memory list">
  <div class="shrink-0 px-3 pb-2 pt-3">
    <label class="context-memory-search">
      <Search size={14} aria-hidden="true" />
      <input type="search" aria-label="Search saved memory" placeholder="Search saved memory…" bind:value={contextShell.search} />
    </label>
    <div class="mt-4 flex items-center justify-between px-1 text-[11px] text-content-quiet"><span>Recent memory</span><span>{entries.length}</span></div>
  </div>
  <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-3" bind:this={listEl}>
    {#if contextThreads.error}
      <p class="px-2 py-3 text-xs text-content-warning">{contextThreads.error} Use Refresh Context to retry.</p>
    {/if}
    {#if !entries.length}
      <p class="px-3 py-5 text-xs leading-relaxed text-content-secondary">{contextThreads.loading ? "Loading saved memory…" : contextShell.search ? "No matching saved memory." : "Saved moments from your sessions will appear here."}</p>
    {/if}
    {#each entries.slice(0, visibleCount) as entry (entry.id)}
      <button type="button" class="context-memory-row" data-memory-id={entry.id} class:selected={contextShell.selectedMapNodeId === entry.id} aria-pressed={contextShell.selectedMapNodeId === entry.id} onclick={() => selectMemory(entry.id)} title={entry.title}>
        <span class="context-memory-title">{entry.title}</span>
        {#if entry.excerpt}<span class="context-memory-excerpt">{entry.excerpt}</span>{/if}
        <span class="context-memory-meta"><span>{entry.kind} · {entry.source.replace(/ · delegated session$/, "")}</span><time>{formatContextWhen(entry.timestamp)}</time></span>
      </button>
    {/each}
    {#if visibleCount < entries.length}
      <button type="button" class="btn btn-sm variant-soft-surface my-2 w-full" onclick={() => shown = visibleCount + 50}>Show more saved memory</button>
    {/if}
  </div>
</section>

<style>
  .context-memory-excerpt { display: -webkit-box; line-clamp: 2; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; margin-top: .35rem; font-size: 11px; line-height: 1.5; color: rgb(var(--theme-text-secondary)); }
  .context-memory-search { display: flex; align-items: center; gap: .5rem; border: 1px solid rgb(var(--color-surface-600) / .35); background: rgb(var(--color-surface-900)); color: rgb(var(--theme-text-quiet)); border-radius: .5rem; padding: .55rem .65rem; }
  .context-memory-search input { width: 100%; min-width: 0; padding: 0; border: 0; background: transparent; color: rgb(var(--color-surface-100)); font-size: 12px; box-shadow: none; }
  .context-memory-search input::placeholder { color: rgb(var(--theme-text-quiet)); }
  .context-memory-row { display: block; width: 100%; padding: .8rem .7rem; margin: .2rem 0; border-radius: .5rem; text-align: left; }
  .context-memory-row:hover { background: rgb(var(--color-surface-800) / .5); }
  .context-memory-row.selected { background: rgb(var(--color-surface-700) / .4); }
  .context-memory-title { display: -webkit-box; line-clamp: 2; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; font-size: 13px; font-weight: 500; line-height: 1.5; color: rgb(var(--color-surface-100)); }
  .context-memory-meta { display: flex; flex-wrap: wrap; gap: .15rem .5rem; margin-top: .35rem; color: rgb(var(--theme-text-secondary)); font-size: 11px; }
  .context-memory-meta > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .context-memory-meta time { color: rgb(var(--theme-text-quiet)); }
</style>
