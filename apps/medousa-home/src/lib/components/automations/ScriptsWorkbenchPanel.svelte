<script lang="ts">
  import {
    Blocks,
    FileCode2,
    Package,
    PanelLeftClose,
    PanelLeftOpen,
    PanelRightClose,
    PanelRightOpen,
  } from "@lucide/svelte";
  import GraphemeRecipeCards from "$lib/components/grapheme/GraphemeRecipeCards.svelte";
  import GraphemeRunResultCard from "$lib/components/grapheme/GraphemeRunResultCard.svelte";
  import GraphemeScriptEditorPanel from "$lib/components/grapheme/GraphemeScriptEditorPanel.svelte";
  import ScriptWorkbenchChatPanel from "$lib/components/automations/ScriptWorkbenchChatPanel.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { applyRecipeToEditor, type GraphemeRecipe } from "$lib/grapheme/graphemeRecipes";
  import { prepareModuleInsert } from "$lib/grapheme/graphemeModuleSnippet";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeModuleSummary, GraphemeScriptEntry } from "$lib/types/grapheme";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();

  type RailSection = "scripts" | "modules" | "wasm";

  let railSection = $state<RailSection>("scripts");
  let leftOpen = $state(true);
  let chatOpen = $state(false);
  let consoleOpen = $state(true);
  let search = $state("");
  let selectedModuleId = $state<string | null>(null);
  let wasmPath = $state("");
  let wasmVersion = $state("");
  let wasmModuleId = $state("");

  $effect(() => {
    if (!visible) return;
    void workshop.refreshModulesAndScripts();
    graphemeScriptEditor.ensureInitialTab();
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
      ? (workshop.modules.find((entry) => entry.module_id === selectedModuleId) ?? null)
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
    void workshop.loadModuleDetail(entry.module_id);
  }

  async function openScript(entry: GraphemeScriptEntry) {
    await graphemeScriptEditor.openScriptById(entry.id);
  }

  function startNewScript() {
    graphemeScriptEditor.openNewTab();
  }

  function startFromRecipe(recipe: GraphemeRecipe) {
    graphemeScriptEditor.openNewTab();
    graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
  }

  function insertOpInEditor(op: string) {
    graphemeScriptEditor.ensureInitialTab();
    const examples = workshop.moduleDetail?.examples ?? [];
    const body = graphemeScriptEditor.activeTab?.body ?? "";
    graphemeScriptEditor.queueInsert(prepareModuleInsert(body, op, examples));
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

  const railItems: { id: RailSection; label: string; icon: typeof FileCode2 }[] = [
    { id: "scripts", label: "Scripts", icon: FileCode2 },
    { id: "modules", label: "Modules", icon: Blocks },
    { id: "wasm", label: "WASM", icon: Package },
  ];
</script>

<div class="scripts-workbench flex min-h-0 flex-1 flex-col overflow-hidden">
  <div class="flex min-h-0 flex-1 overflow-hidden">
    {#if !mobile}
      <nav class="scripts-workbench-rail" aria-label="Workbench tools">
        {#each railItems as item (item.id)}
          {@const Icon = item.icon}
          <button
            type="button"
            class="scripts-workbench-rail-btn {railSection === item.id
              ? 'scripts-workbench-rail-btn-active'
              : ''}"
            title={item.label}
            aria-label={item.label}
            aria-current={railSection === item.id ? "page" : undefined}
            onclick={() => {
              railSection = item.id;
              leftOpen = true;
              search = "";
            }}
          >
            <Icon size={18} strokeWidth={1.75} />
          </button>
        {/each}
      </nav>
    {/if}

    {#if leftOpen || mobile}
      <aside
        class="scripts-workbench-sidebar mobile-you-scroll shrink-0 overflow-y-auto border-r border-surface-500/40 {mobile
          ? 'min-w-0 flex-1'
          : 'w-[min(280px,28%)]'}"
      >
        <div class="flex items-center justify-between gap-2 border-b border-surface-500/35 px-3 py-2">
          {#if mobile}
            <div class="flex gap-1">
              {#each railItems as item (item.id)}
                <button
                  type="button"
                  class="rounded px-2 py-1 text-[10px] {railSection === item.id
                    ? 'bg-surface-800 text-primary-300'
                    : 'text-surface-400'}"
                  onclick={() => (railSection = item.id)}
                >
                  {item.label}
                </button>
              {/each}
            </div>
          {:else}
            <p class="workshop-label">{railItems.find((item) => item.id === railSection)?.label}</p>
            <button
              type="button"
              class="workshop-text-action rounded p-1"
              aria-label="Hide sidebar"
              onclick={() => (leftOpen = false)}
            >
              <PanelLeftClose size={14} strokeWidth={1.75} />
            </button>
          {/if}
        </div>

        <div class="px-3 py-2">
          <input
            class="input w-full text-xs"
            type="search"
            placeholder={railSection === "scripts"
              ? "Search saved scripts…"
              : railSection === "modules"
                ? "Search modules…"
                : "Filter modules…"}
            bind:value={search}
          />
        </div>

        {#if workshop.loading && workshop.modules.length === 0}
          <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
        {:else if workshop.error}
          <p class="px-3 py-2 text-sm text-error-400">{workshop.error}</p>
        {:else if railSection === "scripts"}
          <div class="px-3 pb-2">
            <button type="button" class="workshop-text-action text-xs" onclick={startNewScript}>
              + New script
            </button>
          </div>
          {#if filteredScripts.length === 0}
            <div class="space-y-3 px-3 pb-4">
              <p class="workshop-muted text-xs">No saved scripts yet.</p>
              {#if settings.showWorkshopGuidance}
                <GraphemeRecipeCards compact title="Starters" onselect={startFromRecipe} />
              {/if}
            </div>
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
        {:else if railSection === "modules"}
          <details class="workshop-advanced mx-3 mb-3 rounded border border-surface-500/35 px-2 py-2">
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

          {#if filteredModules.length === 0}
            <p class="workshop-muted px-3 py-2 text-xs">No modules match.</p>
          {:else}
            <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
              {#each filteredModules as entry (entry.module_id)}
                <li>
                  <button
                    type="button"
                    class="flex w-full flex-col px-3 py-2 text-left transition hover:bg-surface-800/70 {selectedModuleId ===
                    entry.module_id
                      ? 'workshop-list-row-active'
                      : ''}"
                    onclick={() => selectModule(entry)}
                  >
                    <span class="font-mono text-sm text-surface-100">{entry.module_id}</span>
                    <span class="workshop-faint mt-0.5 text-[10px] leading-snug">
                      {moduleBlurb(entry)}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          {#if selectedModule && workshop.moduleDetail}
            <div class="border-t border-surface-500/35 px-3 py-3">
              <p class="workshop-label">Actions</p>
              <ul class="mt-2 max-h-56 space-y-2 overflow-y-auto">
                {#each workshop.moduleDetail.info.exported_ops as op (op.op)}
                  <li class="rounded border border-surface-500/35 px-2 py-1.5 text-[11px]">
                    <div class="flex items-center gap-2">
                      <span class="min-w-0 flex-1 truncate font-mono text-surface-100">{op.op}</span>
                      <button
                        type="button"
                        class="workshop-text-action shrink-0"
                        onclick={() => insertOpInEditor(op.op)}
                      >
                        Insert
                      </button>
                    </div>
                    <span class="workshop-faint text-[10px] {effectBadgeClass(op.effect)}">
                      {op.effect}
                    </span>
                  </li>
                {/each}
              </ul>
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
      </aside>
    {/if}

    <div class="scripts-workbench-center flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div class="flex shrink-0 items-center gap-2 border-b border-surface-500/35 px-2 py-1">
        {#if !mobile && !leftOpen}
          <button
            type="button"
            class="workshop-text-action rounded p-1"
            aria-label="Show sidebar"
            onclick={() => (leftOpen = true)}
          >
            <PanelLeftOpen size={14} strokeWidth={1.75} />
          </button>
        {/if}
        <span class="workshop-faint min-w-0 flex-1 truncate text-[11px]">
          {graphemeScriptEditor.activeTab?.name ?? "Workbench"}
        </span>
        {#if !mobile}
          <button
            type="button"
            class="workshop-text-action rounded p-1"
            aria-label="{consoleOpen ? 'Hide' : 'Show'} console"
            onclick={() => (consoleOpen = !consoleOpen)}
          >
            <span class="text-[10px]">Console</span>
          </button>
          <button
            type="button"
            class="workshop-text-action rounded p-1"
            aria-label="{chatOpen ? 'Hide' : 'Show'} script chat"
            onclick={() => (chatOpen = !chatOpen)}
          >
            {#if chatOpen}
              <PanelRightClose size={14} strokeWidth={1.75} />
            {:else}
              <PanelRightOpen size={14} strokeWidth={1.75} />
            {/if}
          </button>
        {/if}
      </div>

      <div class="flex min-h-0 flex-1 overflow-hidden">
        <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <GraphemeScriptEditorPanel {visible} workbenchMode />
          {#if consoleOpen}
            <div class="scripts-workbench-console shrink-0 border-t border-surface-500/40">
              <div class="flex items-center justify-between gap-2 px-3 py-1.5">
                <p class="workshop-label text-[10px]">Output</p>
                <button
                  type="button"
                  class="workshop-text-action text-[10px]"
                  onclick={() => (consoleOpen = false)}
                >
                  Hide
                </button>
              </div>
              <div class="max-h-40 overflow-y-auto px-3 pb-3">
                {#if graphemeScriptEditor.compileError}
                  <p class="text-xs text-error-400">{graphemeScriptEditor.compileError}</p>
                {:else if graphemeScriptEditor.compileResult}
                  <div class="space-y-1 text-[11px] text-surface-300">
                    {#each graphemeScriptEditor.compileResult.compile_hints as hint (hint)}
                      <p>{hint}</p>
                    {/each}
                    {#each graphemeScriptEditor.compileResult.lint_warnings as warning (warning)}
                      <p class="text-warning-400">{warning}</p>
                    {/each}
                  </div>
                {/if}
                <GraphemeRunResultCard
                  result={workshop.runResult?.result}
                  error={workshop.runError}
                  emptyMessage="Run or compile to see output here."
                />
              </div>
            </div>
          {/if}
        </div>

        {#if chatOpen && !mobile}
          <ScriptWorkbenchChatPanel
            visible={visible}
            onOpenFullChat={() => layout.navigateDesktop("chat", { bump: true })}
          />
        {/if}
      </div>
    </div>
  </div>
</div>
