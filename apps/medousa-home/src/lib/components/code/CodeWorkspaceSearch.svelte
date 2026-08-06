<script lang="ts">
  import { onMount, tick } from "svelte";
  import { LoaderCircle, Search, X } from "@lucide/svelte";
  import {
    humanizeForgeMessage,
    searchUndertakingSource,
    type ForgeSourceSearch,
  } from "$lib/forge";

  interface Props {
    workId: string;
    onOpenHit?: (path: string, line: number) => void | Promise<void>;
    onClose?: () => void;
  }

  let { workId, onOpenHit, onClose }: Props = $props();

  let query = $state("");
  let regex = $state(false);
  let caseSensitive = $state(true);
  let wholeWord = $state(false);
  let changedOnly = $state(false);
  let include = $state("");
  let exclude = $state("");
  let loading = $state(false);
  let loadingMore = $state(false);
  let error = $state<string | null>(null);
  let result = $state<ForgeSourceSearch | null>(null);
  let requestEpoch = 0;
  let queryInput: HTMLInputElement | null = $state(null);

  const hits = $derived(result?.hits ?? []);
  const canLoadMore = $derived(Boolean(result?.next_cursor));

  onMount(() => {
    void tick().then(() => queryInput?.focus());
  });

  async function runSearch(options?: { append?: boolean }) {
    const needle = query.trim();
    if (needle.length < 2) {
      error = "Type at least 2 characters";
      return;
    }
    const append = options?.append === true;
    const epoch = ++requestEpoch;
    if (append) loadingMore = true;
    else {
      loading = true;
      result = null;
    }
    error = null;
    try {
      const page = await searchUndertakingSource(workId, {
        query: needle,
        mode: regex ? "regex" : "literal",
        caseSensitive,
        wholeWord,
        include: include.trim() || undefined,
        exclude: exclude.trim() || undefined,
        scope: changedOnly ? "changed" : "all",
        limit: 100,
        cursor: append ? result?.next_cursor : null,
      });
      if (epoch !== requestEpoch) return;
      if (append && result) {
        result = {
          ...page,
          hits: [...result.hits, ...page.hits],
        };
      } else {
        result = page;
      }
    } catch (err) {
      if (epoch !== requestEpoch) return;
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      if (epoch === requestEpoch) {
        loading = false;
        loadingMore = false;
      }
    }
  }

  function cancel() {
    requestEpoch += 1;
    loading = false;
    loadingMore = false;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      void runSearch();
    }
  }
</script>

<div class="flex max-h-80 min-h-[12rem] flex-col border-t border-surface-500/30 bg-surface-950/85">
  <header class="flex shrink-0 items-center gap-2 border-b border-surface-500/20 px-2 py-1.5">
    <Search size={12} class="shrink-0 text-content-quiet" />
    <span class="text-[10px] font-medium uppercase tracking-wide text-content-tertiary">Search</span>
    <div class="ml-auto flex items-center gap-1">
      {#if loading || loadingMore}
        <button
          type="button"
          class="rounded px-1.5 py-0.5 text-[9px] text-content-quiet hover:bg-surface-800"
          onclick={cancel}
        >Cancel</button>
      {/if}
      {#if onClose}
        <button
          type="button"
          class="rounded p-0.5 text-content-quiet hover:text-surface-200"
          aria-label="Close search"
          onclick={onClose}
        ><X size={11} /></button>
      {/if}
    </div>
  </header>

  <div class="flex shrink-0 flex-col gap-1.5 border-b border-surface-500/15 px-2 py-1.5">
    <input
      bind:this={queryInput}
      class="w-full rounded border border-surface-500/35 bg-surface-900/80 px-2 py-1 text-[11px] text-content-secondary outline-none focus:border-primary-500/50"
      placeholder="Search in project…"
      bind:value={query}
      onkeydown={onKeydown}
    />
    <div class="flex flex-wrap items-center gap-1">
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[9px] {caseSensitive ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800'}"
        title="Match case"
        aria-pressed={caseSensitive}
        onclick={() => (caseSensitive = !caseSensitive)}
      >Aa</button>
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[9px] {wholeWord ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800'}"
        title="Whole word"
        aria-pressed={wholeWord}
        onclick={() => (wholeWord = !wholeWord)}
      >W</button>
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[9px] {regex ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800'}"
        title="Use regular expression"
        aria-pressed={regex}
        onclick={() => (regex = !regex)}
      >.*</button>
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[9px] {changedOnly ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800'}"
        title="Changed files only"
        aria-pressed={changedOnly}
        onclick={() => (changedOnly = !changedOnly)}
      >Changed</button>
      <button
        type="button"
        class="ml-auto rounded bg-primary-500/80 px-2 py-0.5 text-[9px] font-medium text-white disabled:opacity-40"
        disabled={loading || query.trim().length < 2}
        onclick={() => void runSearch()}
      >Search</button>
    </div>
    <div class="grid grid-cols-2 gap-1">
      <input
        class="rounded border border-surface-500/25 bg-surface-900/50 px-1.5 py-0.5 text-[9px] text-content-tertiary outline-none focus:border-primary-500/40"
        placeholder="files to include"
        bind:value={include}
        onkeydown={onKeydown}
      />
      <input
        class="rounded border border-surface-500/25 bg-surface-900/50 px-1.5 py-0.5 text-[9px] text-content-tertiary outline-none focus:border-primary-500/40"
        placeholder="files to exclude"
        bind:value={exclude}
        onkeydown={onKeydown}
      />
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if loading}
      <p class="flex items-center gap-1.5 px-3 py-3 text-[10px] text-content-quiet">
        <LoaderCircle size={11} class="animate-spin" /> Searching…
      </p>
    {:else if error}
      <p class="px-3 py-3 text-[10px] text-rose-300/90">{error}</p>
    {:else if !result}
      <p class="px-3 py-3 text-[10px] text-content-quiet">
        Search tracked and untracked source. Replace arrives in a later slice.
      </p>
    {:else if hits.length === 0}
      <p class="px-3 py-3 text-[10px] text-content-quiet">No matches.</p>
    {:else}
      {#each hits as hit, index (`${hit.path}:${hit.line}:${index}`)}
        <button
          type="button"
          class="flex w-full flex-col gap-0.5 border-b border-surface-500/10 px-3 py-1.5 text-left hover:bg-surface-800/60"
          onclick={() => void onOpenHit?.(hit.path, hit.line)}
        >
          <span class="truncate text-[10px] text-content-secondary">
            {hit.path}<span class="text-content-quiet">:{hit.line}</span>
          </span>
          <span class="truncate font-mono text-[9px] text-content-quiet">{hit.preview}</span>
        </button>
      {/each}
      {#if canLoadMore}
        <div class="px-3 py-2">
          <button
            type="button"
            class="rounded px-2 py-1 text-[9px] text-primary-200 hover:bg-primary-900/25 disabled:opacity-40"
            disabled={loadingMore}
            onclick={() => void runSearch({ append: true })}
          >{loadingMore ? "Loading…" : "Load more"}</button>
        </div>
      {:else if result.truncated}
        <p class="px-3 py-2 text-[9px] text-content-quiet">Results truncated.</p>
      {/if}
    {/if}
  </div>
</div>
