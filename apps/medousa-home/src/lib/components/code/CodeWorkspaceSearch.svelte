<script lang="ts">
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import CodePanelFrame from "./CodePanelFrame.svelte";
  import { onMount, onDestroy, tick, untrack } from "svelte";
  import { captureCodeScope, codeExecutionScopeKey } from "$lib/code/codeWorkspaceContext.svelte";
  import { LoaderCircle, Search, X } from "@lucide/svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { buildTextDiff } from "$lib/diff/buildTextDiff";
  import {
    canStartHumanEditing,
    startHumanEditingSession,
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
    workspaceScope: string;
    packageRoot?: string | null;
    onOpenHit?: (path: string, line: number) => void | Promise<void>;
    onClose?: () => void;
    onApplied?: () => void | Promise<void>;
  }

  let { workId, workspaceScope, packageRoot = null, onOpenHit, onClose, onApplied }: Props = $props();

  let query = $state("");
  let replacement = $state("");
  let regex = $state(false);
  let caseSensitive = $state(true);
  let wholeWord = $state(false);
  let scope = $state("project");
  let scopePackage = $state<string | null>(null);
  let replaceOpen = $state(false);
  let advancedOpen = $state(false);
  let cancelled = $state(false);
  let preferencesReady = $state(false);
  let reviewedOptions: ReturnType<typeof searchOptions> & { replacement: string } | null = null;
  let previewEpoch = 0;
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
  let disposed = false;
  let queryInput: HTMLInputElement | null = $state(null);
  let resultsContainer = $state<HTMLDivElement | null>(null);
  let replaceDialog = $state<HTMLDivElement | null>(null);

  const hits = $derived(result?.hits ?? []);
  const groups = $derived.by(() => {
    const grouped = new Map<string, typeof hits>();
    for (const hit of hits) grouped.set(hit.path, [...(grouped.get(hit.path) ?? []), hit]);
    return [...grouped].map(([path, rows]) => ({ path, rows }));
  });
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
    try {
      const saved = JSON.parse(localStorage.getItem(`medousa:code-search:${activeWorkshopId()}:${workId}`) ?? "null");
      if (saved && ["project", "changed", "package"].includes(saved.scope)) {
        scope = saved.scope;
        scopePackage = typeof saved.root === "string" ? saved.root : null;
        if (scope === "package" && !scopePackage) scope = "project";
      }
    } catch { /* Search remains usable without saved preferences. */ }
    preferencesReady = true;
    void tick().then(() => queryInput?.focus());
  });
  onDestroy(() => { disposed = true; requestEpoch += 1; previewEpoch += 1; });

  function captureScope() {
    const current = captureCodeScope(() => JSON.stringify([codeExecutionScopeKey(), workspaceScope, workId]));
    return () => !disposed && current();
  }

  function searchOptions() {
    return {
      query: query.trim(),
      mode: (regex ? "regex" : "literal") as "regex" | "literal",
      caseSensitive,
      wholeWord,
      include: scope === "package" && scopePackage && scopePackage !== "."
        ? include.trim() ? include.split(",").map((glob) => `${scopePackage}/${glob.trim()}`).join(",") : `:(literal)${scopePackage}`
        : include.trim() || undefined,
      exclude: exclude.trim() || undefined,
      scope: (scope === "changed" ? "changed" : "all") as "changed" | "all",
    };
  }

  async function runSearch(options?: { append?: boolean }) {
    const current = captureScope();
    const needle = query.trim();
    if (needle.length < 1) {
      error = "Type a search query";
      return;
    }
    const append = options?.append === true;
    const epoch = ++requestEpoch;
    cancelled = false;
    if (append) loadingMore = true;
    else {
      loading = true;
      result = null;
    }
    error = null;
    try {
      const page = await searchUndertakingSource(workId, {
        ...searchOptions(),
        limit: needle.length === 1 ? 50 : 100,
        cursor: append ? result?.next_cursor : null,
      });
      if (!current() || epoch !== requestEpoch) return;
      if (append && result) {
        result = {
          ...page,
          hits: [...result.hits, ...page.hits],
        };
      } else {
        result = page;
      }
    } catch (err) {
      if (!current() || epoch !== requestEpoch) return;
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      if (current() && epoch === requestEpoch) {
        loading = false;
        loadingMore = false;
      }
    }
  }

  async function previewReplace() {
    const current = captureScope();
    const needle = query.trim();
    if (needle.length < 1) {
      error = "Type a search query to replace";
      return;
    }
    const epoch = ++previewEpoch;
    const reviewed = { ...searchOptions(), replacement };
    previewing = true;
    error = null;
    try {
      const plan = await replaceUndertakingSource(workId, {
        ...reviewed,
        dryRun: true,
        limit: 50,
      });
      if (!current() || epoch !== previewEpoch) return;
      reviewedOptions = reviewed;
      excludedPaths = new Set();
      replacePlan = plan;
      if (plan.files.length === 0) {
        error = "No replaceable matches in the current search scope.";
        replacePlan = null;
      }
    } catch (err) {
      if (!current() || epoch !== previewEpoch) return;
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      if (current() && epoch === previewEpoch) previewing = false;
    }
  }

  async function applyReplace() {
    const current = captureScope();
    const plan = replacePlan;
    const reviewed = reviewedOptions;
    const selectedFiles = [...replaceFiles];
    const previewCurrent = () => current() && replacePlan === plan && reviewedOptions === reviewed;
    if (!plan || !reviewed || applying || selectedFiles.length === 0) return;
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
        const detail = undertakings.detail;
        if (detail?.id !== workId || !canStartHumanEditing(detail.allowed_actions)) {
          throw new Error(
            detail?.allowed_actions.continue_editing?.reason
              ?? detail?.allowed_actions.begin_attempt.reason
              ?? "This project is not ready for file changes",
          );
        }
        const begun = await startHumanEditingSession(workId, detail.allowed_actions);
        if (!previewCurrent()) return;
        undertakings.setActiveFromItem(begun.item, {
          leaseId: begun.lease.lease_id,
          leaseGeneration: begun.lease.generation,
          executorKind: "human",
        });
        leaseId = begun.lease.lease_id;
        generation = begun.lease.generation;
      }
      await replaceUndertakingSource(workId, {
        ...reviewed,
        dryRun: false,
        paths: selectedFiles.map((file) => file.path),
        preconditions: selectedFiles.map((file) => ({
          path: file.path,
          expected_digest: file.expected_digest,
        })),
        lease_id: leaseId,
        generation,
        limit: 50,
      });
      if (!current()) return;
      replacePlan = null;
      excludedPaths = new Set();
      await runSearch();
      if (!current()) return;
      await onApplied?.();
    } catch (err) {
      if (!current()) return;
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      if (current()) applying = false;
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
    cancelled = true;
    loading = false;
    loadingMore = false;
  }

  let debounce: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const signature = JSON.stringify(searchOptions());
    void signature;
    if (preferencesReady) try { localStorage.setItem(`medousa:code-search:${activeWorkshopId()}:${workId}`, JSON.stringify({ scope, root: scopePackage })); } catch { /* Preferences do not grant runtime authority. */ }
    requestEpoch += 1; previewEpoch += 1;
    result = null; replacePlan = null; reviewedOptions = null;
    loading = false; loadingMore = false; previewing = false; cancelled = false; error = null;
    if (debounce) clearTimeout(debounce);
    if (query.trim()) debounce = setTimeout(() => untrack(() => void runSearch()), 250);
    return () => { if (debounce) clearTimeout(debounce); };
  });
  $effect(() => {
    void replacement; void JSON.stringify(searchOptions());
    previewEpoch += 1; replacePlan = null; reviewedOptions = null; previewing = false;
  });
  $effect(() => {
    if (replacePlan) void tick().then(() => replaceDialog?.focus());
  });
  function resultKeys(event: KeyboardEvent) {
    if (event.key === "Escape") { event.preventDefault(); onClose?.(); return; }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const rows = Array.from(resultsContainer?.querySelectorAll<HTMLButtonElement>("[data-search-hit]") ?? []);
    const index = rows.indexOf(event.currentTarget as HTMLButtonElement);
    if (event.key === "ArrowUp" && index === 0) queryInput?.focus();
    else rows[Math.max(0, Math.min(rows.length - 1, index + (event.key === "ArrowDown" ? 1 : -1)))]?.focus();
  }
  function dialogKeys(event: KeyboardEvent) {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); if (!applying) { replacePlan = null; queryInput?.focus(); } return; }
    if (event.key !== "Tab") return;
    const controls = Array.from(replaceDialog?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), [tabindex="0"]') ?? []);
    const index = controls.indexOf(document.activeElement as HTMLElement);
    if (event.shiftKey && index <= 0) { event.preventDefault(); controls.at(-1)?.focus(); }
    else if (!event.shiftKey && (index < 0 || index === controls.length - 1)) { event.preventDefault(); controls[0]?.focus(); }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault(); if (debounce) clearTimeout(debounce); void runSearch();
    } else if (event.key === "ArrowDown") {
      event.preventDefault(); resultsContainer?.querySelector<HTMLButtonElement>("[data-search-hit]")?.focus();
    } else if (event.key === "Escape" && !replacePlan) { event.preventDefault(); onClose?.(); }
  }
  function highlight(preview: string) {
    if (regex || !query.trim()) return { before: preview, match: "", after: "" };
    const start = caseSensitive ? preview.indexOf(query.trim()) : preview.toLocaleLowerCase().indexOf(query.trim().toLocaleLowerCase());
    return start < 0 ? { before: preview, match: "", after: "" } : { before: preview.slice(0, start), match: preview.slice(start, start + query.trim().length), after: preview.slice(start + query.trim().length) };
  }
</script>

<CodePanelFrame {workId} name="Search">
  <header class="flex shrink-0 items-center gap-2 border-b border-surface-500/20 px-2 py-1.5">
    <Search size={12} class="shrink-0 text-content-quiet" />
    <span class="text-chrome-sm font-medium uppercase tracking-wide text-content-tertiary">Search</span>
    <div class="ml-auto flex items-center gap-1">
      {#if loading || loadingMore}
        <button
          type="button"
          class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800"
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
      class="w-full rounded border border-surface-500/35 bg-surface-900/80 px-2 py-1 text-chrome-md text-content-secondary outline-none focus:border-primary-500/50"
      aria-label="Search in project"
      placeholder="Search in project…"
      bind:value={query}
      onkeydown={onKeydown}
    />
    {#if replaceOpen}
    <input
      class="w-full rounded border border-surface-500/35 bg-surface-900/80 px-2 py-1 text-chrome-md text-content-secondary outline-none focus:border-primary-500/50"
      aria-label="Replacement text"
      placeholder="Replace with…"
      bind:value={replacement}
      onkeydown={onKeydown}
    />
    {/if}
    <div class="flex flex-wrap items-center gap-1">
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-chrome-xs {caseSensitive ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800'}"
        title="Match case"
        aria-label="Match case"
        aria-pressed={caseSensitive}
        onclick={() => (caseSensitive = !caseSensitive)}
      >Aa</button>
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-chrome-xs {wholeWord ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800'}"
        title="Whole word"
        aria-label="Whole word"
        aria-pressed={wholeWord}
        onclick={() => (wholeWord = !wholeWord)}
      >W</button>
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-chrome-xs {regex ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800'}"
        title="Use regular expression"
        aria-label="Use regular expression"
        aria-pressed={regex}
        onclick={() => (regex = !regex)}
      >.*</button>
      <select class="rounded bg-surface-900 px-2 py-1 text-chrome-xs" aria-label="Search scope" bind:value={scope} onchange={() => { if (scope === "package") scopePackage = packageRoot; }}><option value="project">Whole project</option><option value="package" disabled={!packageRoot}>Current package</option><option value="changed">Changed files</option></select>
      {#if scope === "package"}<span class="text-chrome-xs text-content-quiet">{scopePackage}</span>{/if}
      <button type="button" class="rounded px-2 py-1 text-chrome-xs text-content-secondary" aria-pressed={replaceOpen} onclick={() => (replaceOpen = !replaceOpen)}>Replace</button>
      <button type="button" class="rounded px-2 py-1 text-chrome-xs text-content-secondary" aria-pressed={advancedOpen} onclick={() => (advancedOpen = !advancedOpen)}>Filters</button>
      <button
        type="button"
        class="ml-auto rounded bg-primary-500/80 px-2 py-0.5 text-chrome-xs font-medium text-white disabled:opacity-40"
        disabled={loading || query.trim().length < 1}
        onclick={() => void runSearch()}
      >Search</button>
      {#if replaceOpen}
      <button
        type="button"
        class="rounded border border-surface-500/40 px-2 py-0.5 text-chrome-xs text-content-secondary hover:bg-surface-800 disabled:opacity-40"
        disabled={previewing || query.trim().length < 1}
        onclick={() => void previewReplace()}
      >{previewing ? "Previewing…" : "Review replace…"}</button>
      {/if}
    </div>
    {#if advancedOpen}
    <div class="grid grid-cols-2 gap-1">
      <input
        class="rounded border border-surface-500/25 bg-surface-900/50 px-1.5 py-0.5 text-chrome-xs text-content-tertiary outline-none focus:border-primary-500/40"
        placeholder="files to include"
        bind:value={include}
        onkeydown={onKeydown}
      />
      <input
        class="rounded border border-surface-500/25 bg-surface-900/50 px-1.5 py-0.5 text-chrome-xs text-content-tertiary outline-none focus:border-primary-500/40"
        placeholder="files to exclude"
        bind:value={exclude}
        onkeydown={onKeydown}
      />
    </div>
    {/if}
  </div>

  <div bind:this={resultsContainer} class="min-h-0 flex-1 overflow-y-auto">
    {#if loading}
      <p class="flex items-center gap-1.5 px-3 py-3 text-chrome-sm text-content-quiet">
        <LoaderCircle size={11} class="animate-spin" /> Searching…
      </p>
    {:else if error && !replacePlan}
      <p class="px-3 py-3 text-chrome-sm text-rose-300/90">{error}</p>
    {:else if cancelled}
      <p class="px-3 py-3 text-chrome-sm text-content-quiet">Search cancelled. Change the query or choose Search to retry.</p>
    {:else if !result}
      <p class="px-3 py-3 text-chrome-sm text-content-quiet">
        Search tracked and untracked source. Preview a replace before applying.
      </p>
    {:else if hits.length === 0}
      <p class="px-3 py-3 text-chrome-sm text-content-quiet">No matches.</p>
    {:else}
      <p class="px-3 py-2 text-chrome-xs text-content-quiet">{hits.length} matching lines in {groups.length} files{result.truncated ? " · more results available" : ""}</p>
      {#each groups as group (group.path)}
        <details open><summary class="sticky top-0 cursor-pointer bg-surface-900 px-3 py-2 font-mono text-chrome-xs text-content-secondary">{group.path} · {group.rows.length}</summary>
          {#each group.rows as hit, index (`${hit.line}:${index}`)}
            {@const parts = highlight(hit.preview)}
            <button type="button" data-search-hit onkeydown={resultKeys} class="flex w-full gap-3 border-b border-surface-500/10 px-3 py-2 text-left hover:bg-surface-800/60 focus:bg-surface-800" onclick={() => void onOpenHit?.(hit.path, hit.line)}>
              <span class="text-chrome-xs text-content-quiet">{hit.line}</span>
              <span class="break-all font-mono text-chrome-xs text-content-secondary">{parts.before}{#if parts.match}<mark class="rounded bg-primary-500/25 text-primary-100">{parts.match}</mark>{/if}{parts.after}</span>
            </button>
          {/each}
        </details>
      {/each}
      {#if canLoadMore}
        <div class="px-3 py-2">
          <button
            type="button"
            class="rounded px-2 py-1 text-chrome-xs text-primary-200 hover:bg-primary-900/25 disabled:opacity-40"
            disabled={loadingMore}
            onclick={() => void runSearch({ append: true })}
          >{loadingMore ? "Loading…" : "Load more"}</button>
        </div>
      {:else if result.truncated}
        <p class="px-3 py-2 text-chrome-xs text-content-quiet">Results truncated.</p>
      {/if}
    {/if}
  </div>
</CodePanelFrame>

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
      bind:this={replaceDialog}
      onkeydown={dialogKeys}
      role="dialog"
      aria-modal="true"
      aria-label="Review replace"
      aria-busy={applying}
      tabindex="-1"
    >
      <header class="flex items-start justify-between gap-3 border-b border-surface-500/30 px-4 py-3">
        <div class="min-w-0">
          <p class="text-sm font-medium text-surface-100">Review replace</p>
          <p class="mt-0.5 text-chrome-sm leading-relaxed text-content-quiet">
            Uncheck files to skip them. Apply verifies every digest and writes the remaining edits atomically.
          </p>
          <div class="mt-2 flex flex-wrap gap-1">
            {#each replacePlan.files as file (file.path)}
              <button
                type="button"
                class="rounded px-1.5 py-0.5 text-chrome-xs {excludedPaths.has(file.path) ? 'bg-surface-800 text-content-quiet line-through' : 'bg-primary-950/50 text-primary-100'}"
                disabled={applying}
                onclick={() => toggleExcluded(file.path)}
              >{file.path} · {file.match_count}</button>
            {/each}
          </div>
          {#if replacePlan.truncated}
            <p class="mt-2 text-chrome-xs text-amber-200/90">Replace plan was truncated to the file limit.</p>
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
        <p class="shrink-0 border-b border-amber-500/30 bg-amber-950/25 px-4 py-2 text-chrome-sm text-amber-100">{error}</p>
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
        <p class="text-chrome-xs text-content-quiet">{replaceFiles.length} of {replacePlan.files.length} files selected</p>
        <div class="flex shrink-0 items-center gap-2">
          <button
            type="button"
            class="rounded px-2.5 py-1.5 text-chrome-sm text-content-tertiary hover:bg-surface-800 disabled:opacity-40"
            disabled={applying}
            onclick={() => (replacePlan = null)}
          >Cancel</button>
          <button
            type="button"
            class="inline-flex items-center gap-1.5 rounded bg-primary-500/80 px-2.5 py-1.5 text-chrome-sm font-medium text-white hover:bg-primary-500 disabled:opacity-40"
            disabled={applying || replaceFiles.length === 0}
            onclick={() => void applyReplace()}
          >{#if applying}<LoaderCircle size={11} class="animate-spin" />Applying…{:else}Apply replace{/if}</button>
        </div>
      </footer>
    </div>
  </div>
{/if}
