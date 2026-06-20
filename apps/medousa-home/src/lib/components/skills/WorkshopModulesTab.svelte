<script lang="ts">
  import GraphemeRecipeCards from "$lib/components/grapheme/GraphemeRecipeCards.svelte";
  import GraphemeRunResultCard from "$lib/components/grapheme/GraphemeRunResultCard.svelte";
  import GraphemeScriptEditorPanel from "$lib/components/grapheme/GraphemeScriptEditorPanel.svelte";
  import WorkshopJourneyBanner from "$lib/components/workshop/WorkshopJourneyBanner.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { applyRecipeToEditor, type GraphemeRecipe } from "$lib/grapheme/graphemeRecipes";
  import { prepareModuleInsert } from "$lib/grapheme/graphemeModuleSnippet";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeModuleSummary, GraphemeScriptEntry } from "$lib/types/grapheme";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();

  let search = $state("");
  let subTab = $state<"modules" | "scripts" | "editor">("modules");
  let selectedModuleId = $state<string | null>(null);
  let selectedScriptId = $state<string | null>(null);
  let wasmPath = $state("");
  let wasmVersion = $state("");

  const mobileDetailOpen = $derived(
    mobile && (selectedModuleId !== null || selectedScriptId !== null),
  );

  $effect(() => {
    if (visible) {
      void workshop.refreshModulesAndScripts();
    }
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

  const selectedModule = $derived(
    selectedModuleId
      ? (workshop.modules.find((entry) => entry.module_id === selectedModuleId) ??
        null)
      : null,
  );

  const selectedScript = $derived(
    selectedScriptId
      ? (workshop.scripts.find((entry) => entry.id === selectedScriptId) ?? null)
      : null,
  );

  const moduleLifecycleEvents = $derived(
    selectedModuleId
      ? workshop.lifecycleEvents.filter(
          (event) =>
            event.module_id.toLowerCase() === selectedModuleId!.toLowerCase(),
        )
      : [],
  );

  function selectModule(entry: GraphemeModuleSummary) {
    selectedModuleId = entry.module_id;
    selectedScriptId = null;
    wasmPath = "";
    wasmVersion = "";
    void workshop.loadModuleDetail(entry.module_id);
  }

  function selectScript(entry: GraphemeScriptEntry) {
    selectedScriptId = entry.id;
    selectedModuleId = null;
    workshop.clearModuleDetail();
  }

  function openEditorForScript(entry: GraphemeScriptEntry) {
    subTab = "editor";
    selectedScriptId = null;
    selectedModuleId = null;
    workshop.clearModuleDetail();
    void graphemeScriptEditor.openScriptById(entry.id);
  }

  function startNewScript() {
    subTab = "editor";
    selectedScriptId = null;
    selectedModuleId = null;
    workshop.clearModuleDetail();
    graphemeScriptEditor.openNewTab();
  }

  function effectBadgeClass(effect: string): string {
    const normalized = String(effect).toLowerCase();
    if (normalized === "network" || normalized === "secrets") {
      return "text-warning-400/80";
    }
    if (normalized === "pure") {
      return "text-surface-500";
    }
    return "text-surface-400";
  }

  async function runSelectedScript(scriptId: string) {
    subTab = "editor";
    await graphemeScriptEditor.openScriptById(scriptId);
    const body = graphemeScriptEditor.activeTab?.body ?? "";
    if (body.trim()) {
      graphemeScriptEditor.sidePane = "diagnostics";
      await workshop.runScriptSource(body);
    }
  }

  function insertOpInEditor(op: string) {
    subTab = "editor";
    graphemeScriptEditor.ensureInitialTab();
    if (selectedModuleId) {
      graphemeScriptEditor.modulesPaneModuleId = selectedModuleId;
    }
    graphemeScriptEditor.sidePane = "modules";
    const examples = workshop.moduleDetail?.examples ?? [];
    const body = graphemeScriptEditor.activeTab?.body ?? "";
    graphemeScriptEditor.queueInsert(prepareModuleInsert(body, op, examples));
  }

  function startFromRecipe(recipe: GraphemeRecipe) {
    subTab = "editor";
    graphemeScriptEditor.openNewTab();
    graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
    graphemeScriptEditor.sidePane = "diagnostics";
  }

  function moduleBlurb(entry: GraphemeModuleSummary): string {
    const blurbs: Record<string, string> = {
      core: "Messages, picking fields, everyday utilities",
      web: "Search the web and fetch pages",
      html: "Parse and convert HTML",
      json: "Read and write JSON data",
      csv: "Spreadsheet-style data",
      yaml: "Config and structured text",
      docs: "Documents and text files",
      io: "Files in and out",
    };
    return (
      blurbs[entry.module_id] ??
      `${entry.op_count} ready-made actions you can insert`
    );
  }
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  {#if !mobileDetailOpen}
    <div class="flex flex-wrap items-center gap-2 px-4 pt-1">
      <button
        type="button"
        class="rounded-md px-2 py-1 text-[11px] transition {subTab === 'modules'
          ? 'bg-surface-700 text-primary-300 ring-1 ring-inset ring-primary-500/35'
          : 'text-surface-400 hover:bg-surface-800 hover:text-surface-200'}"
        onclick={() => {
          subTab = "modules";
          search = "";
        }}
      >
        Modules · {filteredModules.length}
      </button>
      <button
        type="button"
        class="rounded-md px-2 py-1 text-[11px] transition {subTab === 'scripts'
          ? 'bg-surface-700 text-primary-300 ring-1 ring-inset ring-primary-500/35'
          : 'text-surface-400 hover:bg-surface-800 hover:text-surface-200'}"
        onclick={() => {
          subTab = "scripts";
          search = "";
        }}
      >
        Script library · {filteredScripts.length}
      </button>
      <button
        type="button"
        class="rounded-md px-2 py-1 text-[11px] transition {subTab === 'editor'
          ? 'bg-surface-700 text-primary-300 ring-1 ring-inset ring-primary-500/35'
          : 'text-surface-400 hover:bg-surface-800 hover:text-surface-200'}"
        onclick={() => {
          subTab = "editor";
          search = "";
          graphemeScriptEditor.ensureInitialTab();
        }}
      >
        Editor
      </button>
    </div>

    {#if subTab !== "editor"}
      <label class="mt-2 block px-4">
        <span class="sr-only">Search {subTab}</span>
        <input
          class="input w-full max-w-md text-sm"
          type="search"
          placeholder={subTab === "modules"
            ? "Search modules…"
            : "Search saved scripts…"}
          bind:value={search}
        />
      </label>
    {/if}
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    {#if subTab === "editor"}
      <GraphemeScriptEditorPanel {visible} />
    {:else}
      <div
        class="workshop-list-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-3 {mobileDetailOpen
          ? 'hidden'
          : ''}"
      >
        {#if workshop.loading && workshop.modules.length === 0}
          <p class="workshop-muted">Loading Grapheme catalog…</p>
        {:else if workshop.error}
          <p class="text-sm text-error-400">{workshop.error}</p>
        {:else if subTab === "modules"}
          {#if settings.showWorkshopGuidance}
            <WorkshopJourneyBanner />
          {/if}

          <details class="workshop-advanced mt-4 rounded-md border border-surface-500/35 px-3 py-2">
            <summary class="workshop-label cursor-pointer select-none">
              Advanced · module allowlist
            </summary>
            <p class="workshop-faint mt-2 text-[11px]">
              Restrict which modules scripts may use. Leave all checked for the full catalog.
            </p>
            {#if workshop.allowlistError}
              <p class="mt-2 text-xs text-error-400">{workshop.allowlistError}</p>
            {/if}
            <ul class="mt-3 max-h-40 space-y-2 overflow-y-auto">
              {#each filteredModules as entry (entry.module_id)}
                <li class="flex items-center gap-2 text-xs">
                  <input
                    id="allow-{entry.module_id}"
                    type="checkbox"
                    checked={workshop.isModuleAllowed(entry.module_id)}
                    disabled={workshop.allowlistBusy}
                    onchange={(event) =>
                      workshop.toggleAllowlistModule(
                        entry.module_id,
                        (event.currentTarget as HTMLInputElement).checked,
                      )}
                  />
                  <label for="allow-{entry.module_id}" class="font-mono text-surface-200">
                    {entry.module_id}
                  </label>
                </li>
              {/each}
            </ul>
          </details>

          <p class="workshop-label mt-5">Building blocks</p>
          <p class="workshop-faint mt-1 text-[11px]">
            Tap a module to see actions you can insert into a script.
          </p>

          {#if filteredModules.length === 0}
            <p class="workshop-muted">No modules match your search.</p>
          {:else}
            <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
              {#each filteredModules as entry (entry.module_id)}
                <li>
                  <button
                    type="button"
                    class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedModuleId ===
                    entry.module_id
                      ? 'workshop-list-row-active'
                      : ''}"
                    onclick={() => selectModule(entry)}
                  >
                    <div class="min-w-0 flex-1">
                      <div class="flex flex-wrap items-center gap-2">
                        <p class="truncate font-medium text-surface-100">
                          {entry.module_id}
                        </p>
                        {#if workshop.allowlistEnforce && !workshop.isModuleAllowed(entry.module_id)}
                          <span class="text-[10px] uppercase tracking-wide text-warning-400">
                            restricted
                          </span>
                        {/if}
                      </div>
                      <p class="workshop-faint mt-0.5 text-[11px] leading-relaxed">
                        {moduleBlurb(entry)}
                      </p>
                    </div>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {:else if filteredScripts.length === 0}
          <div class="space-y-4">
            <p class="workshop-muted">
              {search.trim()
                ? "No scripts match your search."
                : "No saved scripts yet."}
            </p>
            {#if settings.showWorkshopGuidance}
              <GraphemeRecipeCards compact title="Starter recipes" onselect={startFromRecipe} />
            {/if}
          </div>
        {:else}
          <div class="mb-3">
            <button type="button" class="workshop-text-action text-sm" onclick={startNewScript}>
              New script
            </button>
          </div>
          <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
            {#each filteredScripts as entry (entry.id)}
              <li>
                <button
                  type="button"
                  class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedScriptId ===
                  entry.id
                    ? 'workshop-list-row-active'
                    : ''}"
                  onclick={() => selectScript(entry)}
                >
                  <div class="min-w-0 flex-1">
                    <p class="truncate font-medium text-surface-100">{entry.name}</p>
                    <p class="workshop-faint mt-0.5 font-mono text-[11px]">
                      {entry.id} · v{entry.version}
                    </p>
                    {#if entry.modules.length > 0}
                      <p class="workshop-faint mt-0.5 truncate text-[11px]">
                        {entry.modules.join(", ")}
                      </p>
                    {/if}
                  </div>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      <aside
        class="{mobile
          ? mobileDetailOpen
            ? 'mobile-you-scroll flex min-h-0 flex-1 flex-col overflow-y-auto'
            : 'hidden'
          : 'workshop-detail-pane w-[min(360px,40%)] shrink-0 overflow-y-auto border-l border-surface-500/40'} px-4 py-4"
      >
        {#if mobileDetailOpen}
          <button
            type="button"
            class="workshop-text-action mb-3 shrink-0 text-sm"
            onclick={() => {
              selectedModuleId = null;
              selectedScriptId = null;
              workshop.clearModuleDetail();
            }}
          >
            ← Back to list
          </button>
        {/if}

        {#if selectedModule && workshop.moduleDetailLoading}
          <p class="workshop-muted text-sm">Loading module detail…</p>
        {:else if selectedModule && workshop.moduleDetailError}
          <p class="text-sm text-warning-400">{workshop.moduleDetailError}</p>
        {:else if selectedModule && workshop.moduleDetail}
          <h2 class="workshop-section-title">What this module does</h2>
          <p class="mt-2 font-medium text-surface-100">
            {workshop.moduleDetail.info.module_id}
          </p>
          <p class="mt-2 text-sm leading-relaxed text-surface-300">
            {moduleBlurb(selectedModule)}
          </p>

          <div class="mt-4">
            <h3 class="workshop-label">Actions you can use</h3>
            <ul class="mt-2 max-h-48 space-y-2 overflow-y-auto">
              {#each workshop.moduleDetail.info.exported_ops as op (op.op)}
                <li class="rounded-md border border-surface-500/35 px-3 py-2 text-xs">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="font-mono text-surface-100">{op.op}</span>
                    <span
                      class="text-[10px] uppercase tracking-wide {effectBadgeClass(op.effect)}"
                    >
                      {op.effect}
                    </span>
                    <span class="text-[10px] uppercase tracking-wide text-surface-500">
                      {op.stability}
                    </span>
                    <button
                      type="button"
                      class="workshop-text-action ml-auto text-[11px]"
                      onclick={() => insertOpInEditor(op.op)}
                    >
                      Insert
                    </button>
                  </div>
                  <p class="workshop-faint mt-1">{op.output_type}</p>
                </li>
              {/each}
            </ul>
          </div>

          <details class="workshop-advanced mt-5 rounded-md border border-surface-500/35 px-3 py-2">
            <summary class="workshop-label cursor-pointer select-none">
              Advanced · WASM & lifecycle
            </summary>
            <div class="mt-3 rounded-md border border-surface-500/35 px-3 py-3">
              <h3 class="workshop-label">WASM hot-load</h3>
            <p class="workshop-faint mt-1 text-[11px]">
              Attach a compiled module generation for in-process activation.
            </p>
            <label class="mt-3 block">
              <span class="workshop-label">Path to .wasm</span>
              <input
                class="input mt-1 w-full font-mono text-[11px]"
                type="text"
                placeholder="/path/to/module.wasm"
                bind:value={wasmPath}
              />
            </label>
            <label class="mt-2 block">
              <span class="workshop-label">Version (optional)</span>
              <input
                class="input mt-1 w-full text-xs"
                type="text"
                placeholder="1.0.0"
                bind:value={wasmVersion}
              />
            </label>
            <button
              type="button"
              class="workshop-text-action mt-3"
              disabled={workshop.moduleLoadBusy || !wasmPath.trim()}
              onclick={() =>
                void workshop.loadWasmModule(
                  selectedModule.module_id,
                  wasmPath.trim(),
                  wasmVersion.trim() || undefined,
                )}
            >
              {workshop.moduleLoadBusy ? "Loading…" : "Load module"}
            </button>
            {#if workshop.moduleLoadError}
              <p class="mt-2 text-xs text-error-400">{workshop.moduleLoadError}</p>
            {:else if workshop.moduleLoadResult}
              <p class="mt-2 text-xs text-surface-300">
                gen {workshop.moduleLoadResult.generation_id} ·
                {workshop.moduleLoadResult.version}
              </p>
            {/if}
          </div>

          <div class="mt-4">
            <div class="flex items-center justify-between gap-2">
              <h3 class="workshop-label">Lifecycle</h3>
              <button
                type="button"
                class="workshop-text-action text-[11px]"
                disabled={workshop.lifecycleLoading}
                onclick={() => void workshop.refreshLifecycle()}
              >
                Refresh
              </button>
            </div>
            {#if workshop.lifecycleError}
              <p class="mt-2 text-xs text-error-400">{workshop.lifecycleError}</p>
            {:else if moduleLifecycleEvents.length === 0}
              <p class="workshop-faint mt-2 text-[11px]">No lifecycle events yet.</p>
            {:else}
              <ul class="mt-2 max-h-40 space-y-2 overflow-y-auto">
                {#each moduleLifecycleEvents as event (`${event.kind}-${event.generation_id}`)}
                  <li class="rounded-md border border-surface-500/35 px-3 py-2 text-[11px]">
                    <p class="font-mono text-surface-100">{event.kind}</p>
                    {#if event.message}
                      <p class="workshop-faint mt-1">{event.message}</p>
                    {/if}
                    {#if event.generation_id != null}
                      <p class="workshop-faint mt-1">gen {event.generation_id}</p>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
          </details>
        {:else if selectedScript}
          <h2 class="workshop-section-title">Script detail</h2>
          <p class="mt-2 font-medium text-surface-100">{selectedScript.name}</p>
          <p class="workshop-faint mt-1 font-mono text-[11px]">{selectedScript.id}</p>
          {#if selectedScript.intent}
            <p class="mt-3 text-sm leading-relaxed text-surface-300">
              {selectedScript.intent}
            </p>
          {/if}
          <dl class="mt-4 space-y-2 text-xs">
            {#if selectedScript.modules.length > 0}
              <div>
                <dt class="workshop-label">Modules</dt>
                <dd class="mt-0.5 text-surface-200">
                  {selectedScript.modules.join(", ")}
                </dd>
              </div>
            {/if}
            {#if selectedScript.tags.length > 0}
              <div>
                <dt class="workshop-label">Tags</dt>
                <dd class="mt-0.5 text-surface-200">{selectedScript.tags.join(", ")}</dd>
              </div>
            {/if}
          </dl>
          <div class="mt-5 flex flex-wrap gap-3">
            <button
              type="button"
              class="workshop-text-action"
              onclick={() => openEditorForScript(selectedScript)}
            >
              Open in editor
            </button>
            <button
              type="button"
              class="workshop-text-action"
              disabled={workshop.runBusy}
              onclick={() => void runSelectedScript(selectedScript.id)}
            >
              {workshop.runBusy ? "Trying…" : "Try it"}
            </button>
          </div>
          {#if workshop.runError}
            <div class="mt-3">
              <GraphemeRunResultCard error={workshop.runError} />
            </div>
          {:else if workshop.runResult}
            <div class="mt-3">
              <GraphemeRunResultCard result={workshop.runResult.result} />
            </div>
          {/if}
        {:else}
          {#if settings.showWorkshopGuidance}
            <GraphemeRecipeCards compact onselect={startFromRecipe} />
          {/if}
          <p class="workshop-muted mt-4 text-sm">
            Pick a module on the left to insert actions into your script.
          </p>
        {/if}
      </aside>
    {/if}
  </div>
</div>
