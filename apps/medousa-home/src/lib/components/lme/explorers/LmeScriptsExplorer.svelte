<script lang="ts">
  import {
    ChevronDown,
    LayoutTemplate,
    Package,
    Plus,
    RefreshCw,
    Search,
    X,
  } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import {
    applyRecipeToEditor,
    GRAPHEME_STARTER_RECIPES,
    type GraphemeRecipe,
  } from "$lib/grapheme/graphemeRecipes";
  import { renameScriptById } from "$lib/grapheme/scriptWorkbenchActions";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { scriptRenameUi } from "$lib/stores/scriptRenameUi.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeScriptEntry } from "$lib/types/grapheme";
  import {
    bindScriptLongPress,
    handleScriptContextMenuEvent,
    shouldSuppressScriptContextMenuClick,
  } from "$lib/utils/scriptContextMenuEvents";

  type DockMenu = "templates" | "wasm" | null;

  let search = $state("");
  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let dockMenu = $state<DockMenu>(null);
  let templateSearch = $state("");
  let templateSearchEl = $state<HTMLInputElement | null>(null);
  let wasmPath = $state("");
  let wasmVersion = $state("");
  let wasmModuleId = $state("");
  let wasmLifecycleOpen = $state(false);
  let wasmModuleOpen = $state(false);
  let libraryRenameDraft = $state("");
  let libraryRenameInput = $state<HTMLInputElement | null>(null);
  let libraryRenameBusy = $state(false);
  let handledLibraryRenameToken = $state(-1);

  const searching = $derived(search.trim().length > 0);
  const refreshing = $derived(workshop.loading || workshop.lifecycleLoading);
  const selectedWasmLabel = $derived(
    wasmModuleId
      ? (workshop.modules.find((entry) => entry.module_id === wasmModuleId)?.module_id ??
        wasmModuleId)
      : "Choose module",
  );
  const canLoadWasm = $derived(
    Boolean(wasmModuleId && wasmPath.trim()) && !workshop.moduleLoadBusy,
  );

  onMount(() => {
    void workshop.refreshModulesAndScripts();
  });

  $effect(() => {
    if (searching && !searchExpanded) {
      searchExpanded = true;
    }
  });

  $effect(() => {
    if (dockMenu !== "templates") return;
    void tick().then(() => templateSearchEl?.focus());
  });

  async function openSearch() {
    closeMenus();
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

  function closeMenus() {
    dockMenu = null;
    templateSearch = "";
    wasmLifecycleOpen = false;
    wasmModuleOpen = false;
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  function toggleMenu(menu: Exclude<DockMenu, null>, event: MouseEvent) {
    event.stopPropagation();
    searchExpanded = false;
    if (dockMenu === menu) {
      closeMenus();
      return;
    }
    dockMenu = menu;
    templateSearch = "";
    wasmLifecycleOpen = false;
    wasmModuleOpen = false;
    if (menu === "wasm" && workshop.modules.length === 0) {
      void workshop.refreshModulesAndScripts();
    }
  }

  function pickWasmModule(moduleId: string) {
    wasmModuleId = moduleId;
    wasmModuleOpen = false;
  }

  async function loadWasm() {
    if (!canLoadWasm) return;
    await workshop.loadWasmModule(
      wasmModuleId,
      wasmPath.trim(),
      wasmVersion.trim() || undefined,
    );
  }

  function refreshLibrary() {
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
      const needle = templateSearch.trim().toLowerCase();
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
    if (shouldSuppressScriptContextMenuClick()) return;
    await lmeWorkspace.openScriptById(entry.id);
  }

  $effect(() => {
    const scriptId = scriptRenameUi.libraryScriptId;
    const token = scriptRenameUi.token;
    if (!scriptId || token === handledLibraryRenameToken) return;
    handledLibraryRenameToken = token;
    const entry = workshop.scripts.find((item) => item.id === scriptId);
    libraryRenameDraft = entry?.name ?? "";
    void tick().then(() => {
      libraryRenameInput?.focus();
      libraryRenameInput?.select();
    });
  });

  async function commitLibraryRename(scriptId: string) {
    if (scriptRenameUi.libraryScriptId !== scriptId || libraryRenameBusy) return;
    const trimmed = libraryRenameDraft.trim() || "Untitled script";
    libraryRenameBusy = true;
    try {
      await renameScriptById(scriptId, trimmed);
    } catch (err) {
      workshop.error = err instanceof Error ? err.message : String(err);
    } finally {
      libraryRenameBusy = false;
      scriptRenameUi.clearLibrary();
    }
  }

  function cancelLibraryRename() {
    scriptRenameUi.clearLibrary();
  }

  function startNewScript() {
    closeMenus();
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
      closeMenus();
      return;
    }
    startFromRecipe(recipe);
    closeMenus();
  }
</script>

<svelte:window onclick={closeMenus} />

<aside class="lme-scripts-explorer flex h-full min-h-0 w-full flex-col" aria-label="Scripts">
  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if workshop.loading && workshop.scripts.length === 0}
      <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
    {:else if workshop.error}
      <p class="px-3 py-2 text-sm text-error-400">{workshop.error}</p>
    {:else if filteredScripts.length === 0}
      <p class="workshop-muted px-3 py-4 text-xs">
        {searching ? "No scripts match." : "No saved scripts yet."}
      </p>
    {:else}
      <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
        {#each filteredScripts as entry (entry.id)}
          <li>
            {#if scriptRenameUi.libraryScriptId === entry.id}
              <div class="flex flex-col gap-0.5 px-3 py-2">
                <input
                  bind:this={libraryRenameInput}
                  class="script-library-rename"
                  type="text"
                  aria-label="Rename script"
                  spellcheck="false"
                  bind:value={libraryRenameDraft}
                  onblur={() => void commitLibraryRename(entry.id)}
                  onkeydown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void commitLibraryRename(entry.id);
                    }
                    if (event.key === "Escape") {
                      event.preventDefault();
                      cancelLibraryRename();
                    }
                  }}
                />
                <span class="workshop-faint truncate font-mono text-[10px]">{entry.id}</span>
              </div>
            {:else}
              <button
                type="button"
                class="flex w-full flex-col px-3 py-2 text-left transition hover:bg-surface-800/70 {graphemeScriptEditor.activeTab?.scriptId ===
                entry.id
                  ? 'workshop-list-row-active'
                  : ''}"
                onclick={() => void openScript(entry)}
                oncontextmenu={(event) =>
                  handleScriptContextMenuEvent(entry.id, entry.name, event)}
                use:bindScriptLongPress={() => ({ scriptId: entry.id, name: entry.name })}
              >
                <span class="truncate text-sm font-medium text-surface-100">{entry.name}</span>
                <span class="workshop-faint mt-0.5 truncate font-mono text-[10px]">
                  {entry.id}
                </span>
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <footer
    class="relative flex shrink-0 items-center gap-1 border-t border-surface-500/25 px-2 py-1.5"
  >
    {#if searchExpanded}
      <div class="lme-dock-search-expand min-w-0 flex-1">
        <Search size={14} strokeWidth={1.75} class="lme-dock-search-glyph" />
        <input
          bind:this={searchInputEl}
          class="lme-dock-search-input"
          type="search"
          placeholder="Search scripts…"
          bind:value={search}
          onkeydown={handleSearchKeydown}
        />
      </div>
    {:else}
      <div class="min-w-0 flex-1"></div>
    {/if}

    <button
      type="button"
      class="vault-dock-icon-btn"
      aria-label="New script"
      title="New"
      onclick={startNewScript}
    >
      <Plus size={16} strokeWidth={1.75} />
    </button>

    <button
      type="button"
      class="vault-dock-icon-btn"
      aria-label="Refresh"
      title="Refresh"
      disabled={refreshing}
      onclick={refreshLibrary}
    >
      <RefreshCw size={15} strokeWidth={1.75} class={refreshing ? "animate-spin" : ""} />
    </button>

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

    <div class="relative shrink-0">
      <button
        type="button"
        class="vault-dock-icon-btn {dockMenu === 'templates'
          ? 'bg-surface-800 text-primary-300'
          : ''}"
        aria-haspopup="dialog"
        aria-expanded={dockMenu === "templates"}
        aria-label="Templates"
        title="Templates"
        onclick={(event) => toggleMenu("templates", event)}
      >
        <LayoutTemplate size={15} strokeWidth={1.75} />
      </button>
      {#if dockMenu === "templates"}
        <div
          class="lme-scripts-popover absolute bottom-full right-0 z-30 mb-1.5 w-[min(19rem,calc(100vw-2rem))]"
          role="dialog"
          aria-label="Templates"
          tabindex="-1"
          onclick={(event) => event.stopPropagation()}
          onkeydown={handleMenuKeydown}
        >
          <div class="lme-scripts-popover-search">
            <Search size={13} class="shrink-0 text-surface-500" />
            <input
              bind:this={templateSearchEl}
              class="lme-scripts-popover-search-input"
              type="search"
              placeholder="Search templates…"
              bind:value={templateSearch}
            />
          </div>
          <ul class="lme-scripts-popover-list" role="listbox" aria-label="Templates">
            {#if filteredRecipes.length === 0}
              <li class="lme-scripts-popover-empty">No matches.</li>
            {:else}
              {#each filteredRecipes as recipe, index (recipe.id)}
                <li class="lme-scripts-popover-item" style="--i: {index}">
                  <button
                    type="button"
                    class="lme-scripts-popover-row"
                    role="option"
                    onclick={() => applyTemplate(recipe)}
                  >
                    <span class="min-w-0 flex-1">
                      <span class="lme-scripts-popover-row-title">{recipe.title}</span>
                      <span class="lme-scripts-popover-row-meta">{recipe.subtitle}</span>
                    </span>
                    <span class="lme-scripts-popover-row-action">Use</span>
                  </button>
                </li>
              {/each}
            {/if}
          </ul>
        </div>
      {/if}
    </div>

    <div class="relative shrink-0">
      <button
        type="button"
        class="vault-dock-icon-btn {dockMenu === 'wasm' ? 'bg-surface-800 text-primary-300' : ''}"
        aria-haspopup="dialog"
        aria-expanded={dockMenu === "wasm"}
        aria-label="WASM modules"
        title="WASM"
        onclick={(event) => toggleMenu("wasm", event)}
      >
        <Package size={15} strokeWidth={1.75} />
      </button>
      {#if dockMenu === "wasm"}
        <div
          class="lme-scripts-popover lme-scripts-popover-wasm absolute bottom-full right-0 z-30 mb-1.5 w-[min(19rem,calc(100vw-2rem))]"
          role="dialog"
          aria-label="Load WASM"
          tabindex="-1"
          onclick={(event) => event.stopPropagation()}
          onkeydown={handleMenuKeydown}
        >
          <div class="lme-scripts-popover-head">
            <div class="min-w-0">
              <p class="lme-scripts-popover-title">WASM</p>
              <p class="lme-scripts-popover-blurb">Drop a module into the runtime</p>
            </div>
            <Package size={16} strokeWidth={1.75} class="lme-scripts-popover-head-icon" />
          </div>

          <div class="lme-scripts-popover-fields">
            <div class="relative">
              <button
                type="button"
                class="lme-scripts-popover-field-btn"
                aria-haspopup="listbox"
                aria-expanded={wasmModuleOpen}
                onclick={() => (wasmModuleOpen = !wasmModuleOpen)}
              >
                <span class="lme-scripts-popover-field-label">Module</span>
                <span class="lme-scripts-popover-field-value {wasmModuleId ? '' : 'is-placeholder'}">
                  {selectedWasmLabel}
                </span>
                <ChevronDown
                  size={13}
                  strokeWidth={2}
                  class="shrink-0 text-surface-500 transition {wasmModuleOpen ? 'rotate-180' : ''}"
                />
              </button>
              {#if wasmModuleOpen}
                <ul
                  class="lme-scripts-popover-module-menu"
                  role="listbox"
                  aria-label="Module id"
                >
                  {#each workshop.modules as entry (entry.module_id)}
                    <li>
                      <button
                        type="button"
                        class="lme-scripts-popover-module-option {wasmModuleId === entry.module_id
                          ? 'is-active'
                          : ''}"
                        role="option"
                        aria-selected={wasmModuleId === entry.module_id}
                        onclick={() => pickWasmModule(entry.module_id)}
                      >
                        {entry.module_id}
                      </button>
                    </li>
                  {:else}
                    <li class="lme-scripts-popover-empty">No modules yet.</li>
                  {/each}
                </ul>
              {/if}
            </div>

            <label class="lme-scripts-popover-field">
              <span class="lme-scripts-popover-field-label">Path</span>
              <input
                class="lme-scripts-popover-field-input font-mono"
                type="text"
                placeholder="/path/to/module.wasm"
                spellcheck="false"
                bind:value={wasmPath}
              />
            </label>

            <label class="lme-scripts-popover-field">
              <span class="lme-scripts-popover-field-label">Version</span>
              <input
                class="lme-scripts-popover-field-input"
                type="text"
                placeholder="optional · 1.0.0"
                bind:value={wasmVersion}
              />
            </label>
          </div>

          {#if workshop.moduleLoadError}
            <p class="lme-scripts-popover-status is-error">{workshop.moduleLoadError}</p>
          {:else if workshop.moduleLoadResult}
            <p class="lme-scripts-popover-status">
              Loaded gen {workshop.moduleLoadResult.generation_id} ·
              {workshop.moduleLoadResult.version}
            </p>
          {/if}

          <div class="lme-scripts-popover-footer">
            <button
              type="button"
              class="lme-scripts-popover-footer-quiet"
              aria-expanded={wasmLifecycleOpen}
              onclick={() => (wasmLifecycleOpen = !wasmLifecycleOpen)}
            >
              Lifecycle
              <ChevronDown
                size={12}
                strokeWidth={2}
                class="transition {wasmLifecycleOpen ? 'rotate-180' : ''}"
              />
            </button>
            <button
              type="button"
              class="lme-scripts-popover-load"
              disabled={!canLoadWasm}
              onclick={() => void loadWasm()}
            >
              {workshop.moduleLoadBusy ? "Loading…" : "Load"}
            </button>
          </div>

          {#if wasmLifecycleOpen}
            <div class="lme-scripts-popover-lifecycle">
              <button
                type="button"
                class="lme-scripts-popover-footer-quiet"
                disabled={workshop.lifecycleLoading}
                onclick={() => void workshop.refreshLifecycle()}
              >
                Refresh events
              </button>
              {#if workshop.lifecycleError}
                <p class="lme-scripts-popover-status is-error">{workshop.lifecycleError}</p>
              {:else if wasmLifecycleEvents.length === 0}
                <p class="lme-scripts-popover-empty px-0">No events yet.</p>
              {:else}
                <ul class="max-h-28 space-y-1 overflow-y-auto">
                  {#each wasmLifecycleEvents as event (`${event.kind}-${event.generation_id}`)}
                    <li class="truncate font-mono text-[10px] text-surface-400">
                      <span class="text-surface-200">{event.kind}</span>
                      {#if event.message}
                        <span class="text-surface-600"> · {event.message}</span>
                      {/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </footer>
</aside>
