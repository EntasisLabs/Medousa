<script lang="ts">
  import { Plus, RefreshCw, Search, X } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import {
    applyRecipeToEditor,
    GRAPHEME_STARTER_RECIPES,
    type GraphemeRecipe,
  } from "$lib/grapheme/graphemeRecipes";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeScriptEntry } from "$lib/types/grapheme";

  let search = $state("");
  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let wasmPath = $state("");
  let wasmVersion = $state("");
  let wasmModuleId = $state("");

  const section = $derived(lmeWorkspace.scriptsExplorerSection);
  const searching = $derived(search.trim().length > 0);
  const refreshing = $derived(workshop.loading || workshop.lifecycleLoading);
  const searchPlaceholder = $derived(
    section === "scripts"
      ? "Search scripts…"
      : section === "templates"
        ? "Search templates…"
        : "Search…",
  );
  const showSearch = $derived(section !== "wasm");
  const showRefresh = $derived(section === "scripts" || section === "wasm");
  const showNew = $derived(section === "scripts");

  onMount(() => {
    void workshop.refreshModulesAndScripts();
  });

  $effect(() => {
    void section;
    search = "";
    searchExpanded = false;
  });

  $effect(() => {
    if (searching && !searchExpanded) {
      searchExpanded = true;
    }
  });

  async function openSearch() {
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchExpanded = false;
    search = "";
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
    }
  }

  function refreshSection() {
    if (section === "wasm") {
      void workshop.refreshLifecycle();
      return;
    }
    void workshop.refreshModulesAndScripts();
  }

  const filteredScripts = $derived(
    workshop.scripts.filter((entry) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return (
        entry.name.toLowerCase().includes(needle) ||
        entry.id.toLowerCase().includes(needle) ||
        entry.modules.some((module) => module.toLowerCase().includes(needle)) ||
        entry.tags.some((tag) => tag.toLowerCase().includes(needle))
      );
    }),
  );

  const filteredRecipes = $derived(
    GRAPHEME_STARTER_RECIPES.filter((recipe) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return (
        recipe.title.toLowerCase().includes(needle) ||
        recipe.subtitle.toLowerCase().includes(needle) ||
        recipe.scriptName.toLowerCase().includes(needle)
      );
    }),
  );

  const wasmLifecycleEvents = $derived(
    wasmModuleId
      ? workshop.lifecycleEvents.filter(
          (event) => event.module_id.toLowerCase() === wasmModuleId.toLowerCase(),
        )
      : workshop.lifecycleEvents,
  );

  async function openScript(entry: GraphemeScriptEntry) {
    await lmeWorkspace.openScriptById(entry.id);
  }

  function startNewScript() {
    lmeWorkspace.openNewScript();
  }

  function startFromRecipe(recipe: GraphemeRecipe) {
    lmeWorkspace.openNewScript();
    graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
    lmeWorkspace.syncScriptTabFromEditor({ activate: true });
  }

  function applyTemplate(recipe: GraphemeRecipe) {
    if (!graphemeScriptEditor.activeTab?.body.trim()) {
      graphemeScriptEditor.ensureInitialTab();
      graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
      lmeWorkspace.syncScriptTabFromEditor({ activate: true });
      return;
    }
    startFromRecipe(recipe);
  }
</script>

<aside class="lme-scripts-explorer flex h-full min-h-0 w-full flex-col" aria-label="Scripts">
  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if workshop.loading && workshop.modules.length === 0 && section !== "templates"}
      <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
    {:else if workshop.error}
      <p class="px-3 py-2 text-sm text-error-400">{workshop.error}</p>
    {:else if section === "templates"}
      {#if filteredRecipes.length === 0}
        <p class="workshop-muted px-3 py-4 text-xs">No templates match.</p>
      {:else}
        <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
          {#each filteredRecipes as recipe (recipe.id)}
            <li>
              <button
                type="button"
                class="scripts-workbench-template-row flex w-full flex-col px-3 py-2.5 text-left transition hover:bg-surface-800/70"
                onclick={() => applyTemplate(recipe)}
              >
                <span class="text-sm font-medium text-surface-100">{recipe.title}</span>
                <span class="workshop-faint mt-0.5 text-[11px] leading-snug">
                  {recipe.subtitle}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if section === "scripts"}
      {#if filteredScripts.length === 0}
        <p class="workshop-muted px-3 py-4 text-xs">
          {searching ? "No scripts match." : "No saved scripts yet."}
        </p>
      {:else}
        <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
          {#each filteredScripts as entry (entry.id)}
            <li>
              <button
                type="button"
                class="flex w-full flex-col px-3 py-2 text-left transition hover:bg-surface-800/70 {graphemeScriptEditor.activeTab?.scriptId ===
                entry.id
                  ? 'workshop-list-row-active'
                  : ''}"
                onclick={() => void openScript(entry)}
              >
                <span class="truncate text-sm font-medium text-surface-100">{entry.name}</span>
                <span class="workshop-faint mt-0.5 truncate font-mono text-[10px]">
                  {entry.id}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else}
      <div class="space-y-3 px-3 py-2 pb-4">
        <label class="block">
          <span class="workshop-label">Module id</span>
          <select class="input mt-1 w-full text-xs" bind:value={wasmModuleId}>
            <option value="">Select…</option>
            {#each workshop.modules as entry (entry.module_id)}
              <option value={entry.module_id}>{entry.module_id}</option>
            {/each}
          </select>
        </label>
        <label class="block">
          <span class="workshop-label">Path to .wasm</span>
          <input
            class="input mt-1 w-full font-mono text-[11px]"
            type="text"
            placeholder="/path/to/module.wasm"
            bind:value={wasmPath}
          />
        </label>
        <label class="block">
          <span class="workshop-label">Version</span>
          <input
            class="input mt-1 w-full text-xs"
            type="text"
            placeholder="1.0.0"
            bind:value={wasmVersion}
          />
        </label>
        <button
          type="button"
          class="btn btn-sm variant-soft-primary"
          disabled={workshop.moduleLoadBusy || !wasmPath.trim() || !wasmModuleId}
          onclick={() =>
            void workshop.loadWasmModule(
              wasmModuleId,
              wasmPath.trim(),
              wasmVersion.trim() || undefined,
            )}
        >
          {workshop.moduleLoadBusy ? "Loading…" : "Load WASM"}
        </button>
        {#if workshop.moduleLoadError}
          <p class="text-xs text-error-400">{workshop.moduleLoadError}</p>
        {:else if workshop.moduleLoadResult}
          <p class="text-xs text-surface-300">
            gen {workshop.moduleLoadResult.generation_id} · {workshop.moduleLoadResult.version}
          </p>
        {/if}

        <details class="workshop-advanced mt-2 rounded border border-surface-500/35 px-2 py-2">
          <summary class="workshop-label cursor-pointer text-[10px]">Lifecycle</summary>
          <button
            type="button"
            class="workshop-text-action mt-2 text-[10px]"
            disabled={workshop.lifecycleLoading}
            onclick={() => void workshop.refreshLifecycle()}
          >
            Refresh
          </button>
          {#if workshop.lifecycleError}
            <p class="mt-2 text-xs text-error-400">{workshop.lifecycleError}</p>
          {:else if wasmLifecycleEvents.length === 0}
            <p class="workshop-faint mt-2 text-[10px]">No events yet.</p>
          {:else}
            <ul class="mt-2 max-h-32 space-y-1 overflow-y-auto">
              {#each wasmLifecycleEvents as event (`${event.kind}-${event.generation_id}`)}
                <li class="text-[10px]">
                  <span class="font-mono text-surface-200">{event.kind}</span>
                  {#if event.message}
                    <span class="workshop-faint"> · {event.message}</span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </details>
      </div>
    {/if}
  </div>

  <footer
    class="relative flex shrink-0 items-center gap-1 border-t border-surface-500/25 px-2 py-1.5"
  >
    {#if searchExpanded && showSearch}
      <div class="lme-dock-search-expand min-w-0 flex-1">
        <Search size={14} strokeWidth={1.75} class="lme-dock-search-glyph" />
        <input
          bind:this={searchInputEl}
          class="lme-dock-search-input"
          type="search"
          placeholder={searchPlaceholder}
          bind:value={search}
          onkeydown={handleSearchKeydown}
        />
      </div>
    {:else}
      <div class="min-w-0 flex-1"></div>
    {/if}

    {#if showNew}
      <button
        type="button"
        class="vault-dock-icon-btn"
        aria-label="New script"
        title="New"
        onclick={startNewScript}
      >
        <Plus size={16} strokeWidth={1.75} />
      </button>
    {/if}

    {#if showRefresh}
      <button
        type="button"
        class="vault-dock-icon-btn"
        aria-label="Refresh"
        title="Refresh"
        disabled={refreshing}
        onclick={refreshSection}
      >
        <RefreshCw size={15} strokeWidth={1.75} class={refreshing ? "animate-spin" : ""} />
      </button>
    {/if}

    {#if showSearch}
      {#if searchExpanded}
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Close search"
          title="Close search"
          onclick={closeSearch}
        >
          <X size={15} strokeWidth={1.75} />
        </button>
      {:else}
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Search"
          title="Search"
          onclick={() => void openSearch()}
        >
          <Search size={15} strokeWidth={1.75} />
        </button>
      {/if}
    {/if}
  </footer>
</aside>
