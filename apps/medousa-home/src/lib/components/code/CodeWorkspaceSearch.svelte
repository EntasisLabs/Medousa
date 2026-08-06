<script lang="ts">
  import { onMount, tick } from "svelte";
  import { LoaderCircle, Search, X } from "@lucide/svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { buildTextDiff } from "$lib/diff/buildTextDiff";
  import {
    beginHumanAttempt,
    humanizeForgeMessage,
    replaceUndertakingSource,
    searchUndertakingSource,
    type ForgeSourceReplacePlan,
    type ForgeSourceSearch,
  } from "$lib/forge";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import type { DiffFileSection } from "$lib/diff/diffTypes";

  interface Props {
    workId: string;
    onOpenHit?: (path: string, line: number) => void | Promise<void>;
    onClose?: () => void;
    onApplied?: () => void | Promise<void>;
  }

  let { workId, onOpenHit, onClose, onApplied }: Props = $props();

  let query = $state("");
  let replacement = $state("");
  let regex = $state(false);
  let caseSensitive = $state(true);
  let wholeWord = $state(false);
  let changedOnly = $state(false);
  let include = $state("");
  let exclude = $state("");
  let loading = $state(false);
  let loadingMore = $state(false);
  let previewing = $state(false);
  let applying = $state(false);
  let error = $state<string | null>(null);
  let result = $state<ForgeSourceSearch | null>(null);
  let replacePlan = $state<ForgeSourceReplacePlan | null>(null);
  let excludedPaths = $state<Set<string>>(new Set());
  let replaceDiffMode = $state<"inline" | "side">("side");
  let requestEpoch = 0;
  let queryInput: HTMLInputElement | null = $state(null);

  const hits = $derived(result?.hits ?? []);
  const canLoadMore = $derived(Boolean(result?.next_cursor));
  const replaceFiles = $derived(
    (replacePlan?.files ?? []).filter((file) => !excludedPaths.has(file.path)),
  );
  const replaceDiffFiles = $derived<DiffFileSection[]>(
    replaceFiles.map((file) => ({
      id: file.path,
      path: file.path,
      status: "changed",
      hunks: buildTextDiff(file.before, file.after),
    })),
  );

  onMount(() => {
    void tick().then(() => queryInput?.focus());
  });

  function searchOptions() {
    return {
      query: query.trim(),
      mode: (regex ? "regex" : "literal") as "regex" | "literal",
      caseSensitive,
      wholeWord,
      include: include.trim() || undefined,
      exclude: exclude.trim() || undefined,
      scope: (changedOnly ? "changed" : "all") as "changed" | "all",
    };
  }

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
        ...searchOptions(),
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

  async function previewReplace() {
    const needle = query.trim();
    if (needle.length < 2) {
      error = "Type at least 2 characters to replace";
      return;
    }
    previewing = true;
    error = null;
    try {
      const plan = await replaceUndertakingSource(workId, {
        ...searchOptions(),
        replacement,
        dryRun: true,
        limit: 50,
      });
      excludedPaths = new Set();
      replacePlan = plan;
      if (plan.files.length === 0) {
        error = "No replaceable matches in the current search scope.";
        replacePlan = null;
      }
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      previewing = false;
    }
  }

  async function applyReplace() {
    const plan = replacePlan;
    if (!plan || applying || replaceFiles.length === 0) return;
    applying = true;
    error = null;
    try {
      let leaseId = undertakings.active?.workId === workId
        ? undertakings.active.leaseId
        : null;
      let generation = undertakings.active?.workId === workId
        ? undertakings.active.leaseGeneration
        : null;
      if (!leaseId || generation == null) {
        const begun = await beginHumanAttempt(workId);
        undertakings.setActiveFromItem(begun.item, {
          leaseId: begun.lease.lease_id,
          leaseGeneration: begun.lease.generation,
          executorKind: "human",
        });
        leaseId = begun.lease.lease_id;
        generation = begun.lease.generation;
      }
      await replaceUndertakingSource(workId, {
        ...searchOptions(),
        replacement,
        dryRun: false,
        paths: replaceFiles.map((file) => file.path),
        preconditions: replaceFiles.map((file) => ({
          path: file.path,
          expected_digest: file.expected_digest,
        })),
        lease_id: leaseId,
        generation,
        limit: 50,
      });
      replacePlan = null;
      excludedPaths = new Set();
      await runSearch();
      await onApplied?.();
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      applying = false;
    }
  }

  function toggleExcluded(path: string) {
    const next = new Set(excludedPaths);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    excludedPaths = next;
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
    <input
      class="w-full rounded border border-surface-500/35 bg-surface-900/80 px-2 py-1 text-[11px] text-content-secondary outline-none focus:border-primary-500/50"
      placeholder="Replace with…"
      bind:value={replacement}
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
      <button
        type="button"
        class="rounded border border-surface-500/40 px-2 py-0.5 text-[9px] text-content-secondary hover:bg-surface-800 disabled:opacity-40"
        disabled={previewing || query.trim().length < 2}
        onclick={() => void previewReplace()}
      >{previewing ? "Previewing…" : "Replace…"}</button>
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
    {:else if error && !replacePlan}
      <p class="px-3 py-3 text-[10px] text-rose-300/90">{error}</p>
    {:else if !result}
      <p class="px-3 py-3 text-[10px] text-content-quiet">
        Search tracked and untracked source. Preview a replace before applying.
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

{#if replacePlan}
  <div class="fixed inset-0 z-[128] flex items-center justify-center p-4">
    <button
      type="button"
      class="absolute inset-0 bg-black/60"
      aria-label="Cancel replace"
      disabled={applying}
      onclick={() => {
        if (!applying) replacePlan = null;
      }}
    ></button>
    <div
      class="relative flex max-h-[90vh] w-full max-w-6xl flex-col overflow-hidden rounded-lg border border-surface-500/50 bg-surface-950 shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-label="Review replace"
      aria-busy={applying}
      tabindex="-1"
    >
      <header class="flex items-start justify-between gap-3 border-b border-surface-500/30 px-4 py-3">
        <div class="min-w-0">
          <p class="text-sm font-medium text-surface-100">Review replace</p>
          <p class="mt-0.5 text-[10px] leading-relaxed text-content-quiet">
            Uncheck files to skip them. Apply verifies every digest and writes the remaining edits atomically.
          </p>
          <div class="mt-2 flex flex-wrap gap-1">
            {#each replacePlan.files as file (file.path)}
              <button
                type="button"
                class="rounded px-1.5 py-0.5 text-[9px] {excludedPaths.has(file.path) ? 'bg-surface-800 text-content-quiet line-through' : 'bg-primary-950/50 text-primary-100'}"
                disabled={applying}
                onclick={() => toggleExcluded(file.path)}
              >{file.path} · {file.match_count}</button>
            {/each}
          </div>
          {#if replacePlan.truncated}
            <p class="mt-2 text-[9px] text-amber-200/90">Replace plan was truncated to the file limit.</p>
          {/if}
        </div>
        <button
          type="button"
          class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-100 disabled:opacity-40"
          aria-label="Cancel replace"
          disabled={applying}
          onclick={() => (replacePlan = null)}
        ><X size={14} /></button>
      </header>
      {#if error}
        <p class="shrink-0 border-b border-amber-500/30 bg-amber-950/25 px-4 py-2 text-[10px] text-amber-100">{error}</p>
      {/if}
      <div class="min-h-0 flex-1 overflow-auto px-4 py-3">
        <DiffStack
          files={replaceDiffFiles}
          bind:mode={replaceDiffMode}
          showJumpList={true}
          busy={applying}
          title="Proposed replacements"
          subtitle="Skipped files stay unchanged. Apply stops if any included file changed since this preview."
          onOpenFile={(path) => void onOpenHit?.(path, 1)}
        />
      </div>
      <footer class="flex items-center justify-between gap-3 border-t border-surface-500/30 px-4 py-3">
        <p class="text-[9px] text-content-quiet">{replaceFiles.length} of {replacePlan.files.length} files selected</p>
        <div class="flex shrink-0 items-center gap-2">
          <button
            type="button"
            class="rounded px-2.5 py-1.5 text-[10px] text-content-tertiary hover:bg-surface-800 disabled:opacity-40"
            disabled={applying}
            onclick={() => (replacePlan = null)}
          >Cancel</button>
          <button
            type="button"
            class="inline-flex items-center gap-1.5 rounded bg-primary-500/80 px-2.5 py-1.5 text-[10px] font-medium text-white hover:bg-primary-500 disabled:opacity-40"
            disabled={applying || replaceFiles.length === 0}
            onclick={() => void applyReplace()}
          >{#if applying}<LoaderCircle size={11} class="animate-spin" />Applying…{:else}Apply replace{/if}</button>
        </div>
      </footer>
    </div>
  </div>
{/if}
