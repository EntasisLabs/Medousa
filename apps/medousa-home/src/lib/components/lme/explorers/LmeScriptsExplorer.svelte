<script lang="ts">
  import { onMount } from "svelte";
  import {
    applyRecipeToEditor,
    GRAPHEME_STARTER_RECIPES,
    type GraphemeRecipe,
  } from "$lib/grapheme/graphemeRecipes";
  import { prepareModuleInsert, qualifyModuleOp } from "$lib/grapheme/graphemeModuleSnippet";
  import {
    effectBadgeClass,
    moduleBlurb,
    stabilityLabel,
  } from "$lib/grapheme/scriptWorkbenchHelpers";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type {
    GraphemeModuleSummary,
    GraphemeScriptEntry,
  } from "$lib/types/grapheme";

  let search = $state("");
  let selectedModuleId = $state<string | null>(null);
  let wasmPath = $state("");
  let wasmVersion = $state("");
  let wasmModuleId = $state("");

  const section = $derived(lmeWorkspace.scriptsExplorerSection);

  onMount(() => {
    void workshop.refreshModulesAndScripts();
  });

  const filteredModules = $derived(
    workshop.modules.filter((entry) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return (
        entry.module_id.toLowerCase().includes(needle) ||
        entry.effects.some((effect) => effect.includes(needle))
      );
    }),
  );

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

  $effect(() => {
    if (section !== "modules") return;
    if (filteredModules.length === 0) {
      selectedModuleId = null;
      return;
    }
    if (
      !selectedModuleId ||
      !filteredModules.some((entry) => entry.module_id === selectedModuleId)
    ) {
      selectedModuleId = filteredModules[0]!.module_id;
      void workshop.loadModuleDetail(selectedModuleId);
    }
  });

  const selectedModule = $derived(
    selectedModuleId
      ? (filteredModules.find((entry) => entry.module_id === selectedModuleId) ?? null)
      : null,
  );

  const moduleDetailForSelected = $derived(
    selectedModuleId && workshop.moduleDetail?.info.module_id === selectedModuleId
      ? workshop.moduleDetail
      : null,
  );

  const filteredModuleOps = $derived.by(() => {
    const ops = moduleDetailForSelected?.info.exported_ops ?? [];
    const needle = search.trim().toLowerCase();
    if (!needle) return ops;
    return ops.filter(
      (op) =>
        op.op.toLowerCase().includes(needle) ||
        op.effect.toLowerCase().includes(needle) ||
        op.output_type.toLowerCase().includes(needle),
    );
  });

  const moduleLifecycleEvents = $derived(
    selectedModuleId
      ? workshop.lifecycleEvents.filter(
          (event) =>
            event.module_id.toLowerCase() === selectedModuleId!.toLowerCase(),
        )
      : [],
  );

  function selectModule(entry: GraphemeModuleSummary) {
    if (selectedModuleId === entry.module_id) return;
    selectedModuleId = entry.module_id;
    void workshop.loadModuleDetail(entry.module_id);
  }

  function insertOpInEditor(op: string) {
    graphemeScriptEditor.ensureInitialTab();
    const examples = workshop.moduleDetail?.examples ?? [];
    const body = graphemeScriptEditor.activeTab?.body ?? "";
    const qualified = qualifyModuleOp(selectedModuleId, op);
    graphemeScriptEditor.queueInsert(prepareModuleInsert(body, qualified, examples));
    lmeWorkspace.syncScriptTabFromEditor({ activate: true });
  }

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
  <div class="px-3 py-2">
    <input
      class="input w-full text-xs"
      type="search"
      placeholder={section === "scripts"
        ? "Search saved scripts…"
        : section === "templates"
          ? "Search templates…"
          : section === "modules"
            ? "Search modules or actions…"
            : "Filter modules…"}
      bind:value={search}
    />
  </div>

  <div
    class="mobile-you-scroll min-h-0 flex-1 {section === 'modules'
      ? 'flex flex-col overflow-hidden'
      : 'overflow-y-auto'}"
  >
    {#if workshop.loading && workshop.modules.length === 0}
      <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
    {:else if workshop.error}
      <p class="px-3 py-2 text-sm text-error-400">{workshop.error}</p>
    {:else if section === "templates"}
      <p class="workshop-faint px-3 pb-2 text-[11px] leading-relaxed">
        Starter scripts — click to load in the editor.
      </p>
      {#if filteredRecipes.length === 0}
        <p class="workshop-muted px-3 py-2 text-xs">No templates match.</p>
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
      <div class="px-3 pb-2">
        <button type="button" class="workshop-text-action text-xs" onclick={startNewScript}>
          + New script
        </button>
      </div>
      {#if filteredScripts.length === 0}
        <p class="workshop-muted px-3 pb-4 text-xs">No saved scripts yet.</p>
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
    {:else if section === "modules"}
      <details class="workshop-advanced mx-3 mb-2 shrink-0 rounded border border-surface-500/35 px-2 py-2">
        <summary class="workshop-label cursor-pointer select-none text-[10px]">
          Module allowlist
        </summary>
        <p class="workshop-faint mt-2 text-[10px]">
          Restrict which modules scripts may use at runtime.
        </p>
        {#if workshop.allowlistError}
          <p class="mt-2 text-xs text-error-400">{workshop.allowlistError}</p>
        {/if}
        <ul class="mt-2 max-h-32 space-y-2 overflow-y-auto">
          {#each filteredModules as entry (entry.module_id)}
            <li class="flex items-center gap-2 text-[10px]">
              <input
                id="lme-allow-{entry.module_id}"
                type="checkbox"
                checked={workshop.isModuleAllowed(entry.module_id)}
                disabled={workshop.allowlistBusy}
                onchange={(event) =>
                  workshop.toggleAllowlistModule(
                    entry.module_id,
                    (event.currentTarget as HTMLInputElement).checked,
                  )}
              />
              <label for="lme-allow-{entry.module_id}" class="font-mono text-surface-200">
                {entry.module_id}
              </label>
            </li>
          {/each}
        </ul>
      </details>

      {#if filteredModules.length === 0}
        <p class="workshop-muted px-3 py-2 text-xs">No modules match.</p>
      {:else}
        <div class="scripts-workbench-modules-split flex min-h-0 flex-1 flex-col overflow-hidden">
          <ul
            class="scripts-workbench-module-list max-h-[min(11rem,38%)] shrink-0 overflow-y-auto divide-y divide-surface-500/35 border-y border-surface-500/35"
            role="listbox"
            aria-label="Modules"
          >
            {#each filteredModules as entry (entry.module_id)}
              <li role="presentation">
                <button
                  type="button"
                  role="option"
                  aria-selected={selectedModuleId === entry.module_id}
                  class="scripts-workbench-module-pick flex w-full items-center gap-2 px-3 py-1.5 text-left transition hover:bg-surface-800/70 {selectedModuleId ===
                  entry.module_id
                    ? 'workshop-list-row-active'
                    : ''}"
                  onclick={() => selectModule(entry)}
                >
                  <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-surface-100">
                    {entry.module_id}
                  </span>
                  <span class="workshop-faint shrink-0 text-[10px] tabular-nums">
                    {entry.op_count}
                  </span>
                </button>
              </li>
            {/each}
          </ul>

          <div class="scripts-workbench-module-detail mobile-you-scroll min-h-0 flex-1 overflow-y-auto">
            {#if selectedModule}
              <div class="border-b border-surface-500/35 px-3 py-2.5">
                <p class="font-mono text-sm font-medium text-surface-50">
                  {selectedModule.module_id}
                </p>
                <p class="workshop-faint mt-1 text-[11px] leading-snug">
                  {moduleBlurb(selectedModule)}
                </p>
                <div class="mt-2 flex flex-wrap gap-1">
                  <span class="scripts-workbench-meta-chip">v{selectedModule.version}</span>
                  <span class="scripts-workbench-meta-chip">
                    {selectedModule.op_count} ops
                  </span>
                  {#each selectedModule.effects as effect (effect)}
                    <span class="scripts-workbench-effect-chip {effectBadgeClass(effect)}">
                      {effect}
                    </span>
                  {/each}
                </div>
              </div>

              {#if workshop.moduleDetailLoading && !moduleDetailForSelected}
                <p class="workshop-muted px-3 py-3 text-[11px]">Loading actions…</p>
              {:else if workshop.moduleDetailError}
                <p class="px-3 py-3 text-[11px] text-error-400">{workshop.moduleDetailError}</p>
              {:else if filteredModuleOps.length === 0}
                <p class="workshop-muted px-3 py-3 text-[11px]">No actions match.</p>
              {:else}
                <ul class="space-y-2 p-2">
                  {#each filteredModuleOps as op (op.op)}
                    <li>
                      <button
                        type="button"
                        class="scripts-workbench-op-card group w-full rounded-md border border-surface-500/35 px-2.5 py-2 text-left transition hover:border-primary-500/30 hover:bg-surface-800/60"
                        onclick={() => insertOpInEditor(op.op)}
                      >
                        <div class="flex items-start justify-between gap-2">
                          <p class="min-w-0 truncate font-mono text-[11px] text-surface-100">
                            {op.op}()
                          </p>
                          <span
                            class="shrink-0 text-[10px] text-surface-500 opacity-0 transition group-hover:opacity-100"
                          >
                            Insert
                          </span>
                        </div>
                        <p class="workshop-faint mt-1 truncate text-[10px]">→ {op.output_type}</p>
                        <div class="mt-1.5 flex flex-wrap gap-1">
                          <span class="scripts-workbench-effect-chip {effectBadgeClass(op.effect)}">
                            {op.effect}
                          </span>
                          <span class="scripts-workbench-meta-chip">{stabilityLabel(op)}</span>
                        </div>
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            {:else}
              <p class="workshop-muted px-3 py-4 text-xs">Select a module above.</p>
            {/if}
          </div>
        </div>
      {/if}
    {:else}
      <div class="space-y-3 px-3 pb-4">
        <p class="workshop-faint text-[11px] leading-relaxed">
          Drop-in WASM extensions for the Grapheme runtime — separate from native modules.
        </p>
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
          {:else if moduleLifecycleEvents.length === 0}
            <p class="workshop-faint mt-2 text-[10px]">No events yet.</p>
          {:else}
            <ul class="mt-2 max-h-32 space-y-1 overflow-y-auto">
              {#each moduleLifecycleEvents as event (`${event.kind}-${event.generation_id}`)}
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
</aside>
