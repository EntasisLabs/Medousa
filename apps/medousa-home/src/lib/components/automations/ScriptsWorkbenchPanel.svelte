<script lang="ts">
  import {
    FileCode2,
    LayoutTemplate,
    Package,
    PanelLeftClose,
  } from "@lucide/svelte";
  import ScriptWorkbenchChatPanel from "$lib/components/automations/ScriptWorkbenchChatPanel.svelte";
  import ScriptWorkbenchConsole from "$lib/components/automations/ScriptWorkbenchConsole.svelte";
  import ScriptWorkbenchOutputSheet from "$lib/components/automations/ScriptWorkbenchOutputSheet.svelte";
  import ScriptWorkbenchStatusBar from "$lib/components/automations/ScriptWorkbenchStatusBar.svelte";
  import ScriptWorkbenchTitlebar from "$lib/components/automations/ScriptWorkbenchTitlebar.svelte";
  import ScriptWorkbenchToolsSheet from "$lib/components/automations/ScriptWorkbenchToolsSheet.svelte";
  import GraphemeScriptEditorPanel from "$lib/components/grapheme/GraphemeScriptEditorPanel.svelte";
  import { applyRecipeToEditor, GRAPHEME_STARTER_RECIPES, type GraphemeRecipe } from "$lib/grapheme/graphemeRecipes";
  import { renameScriptById } from "$lib/grapheme/scriptWorkbenchActions";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { scriptRenameUi } from "$lib/stores/scriptRenameUi.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeScriptEntry } from "$lib/types/grapheme";
  import {
    bindScriptLongPress,
    handleScriptContextMenuEvent,
    shouldSuppressScriptContextMenuClick,
  } from "$lib/utils/scriptContextMenuEvents";
  import { tick } from "svelte";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();

  type RailSection = "scripts" | "templates" | "wasm";

  let railSection = $state<RailSection>("scripts");
  let leftOpen = $state(true);
  let chatOpen = $state(false);
  let consoleOpen = $state(true);
  let search = $state("");
  let wasmPath = $state("");
  let wasmVersion = $state("");
  let wasmModuleId = $state("");
  let toolsSheetOpen = $state(false);
  let outputSheetOpen = $state(false);
  let toolsInitialView = $state<"root" | "templates" | "library" | "chat">("root");
  let libraryRenameDraft = $state("");
  let libraryRenameInput = $state<HTMLInputElement | null>(null);
  let libraryRenameBusy = $state(false);
  let handledLibraryRenameToken = $state(-1);

  const showMobileEmptyHint = $derived(
    mobile &&
      Boolean(
        graphemeScriptEditor.activeTab && !graphemeScriptEditor.activeTab.body.trim(),
      ),
  );

  function openTools(view: typeof toolsInitialView = "root") {
    toolsInitialView = view;
    toolsSheetOpen = true;
  }

  function openOutput() {
    outputSheetOpen = true;
  }

  $effect(() => {
    if (!visible) return;
    void workshop.refreshModulesAndScripts();
    graphemeScriptEditor.ensureInitialTab();
  });

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

  function applyTemplate(recipe: GraphemeRecipe) {
    if (!graphemeScriptEditor.activeTab?.body.trim()) {
      graphemeScriptEditor.ensureInitialTab();
      graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
      return;
    }
    startFromRecipe(recipe);
  }

  const railItems: { id: RailSection; label: string; icon: typeof FileCode2 }[] = [
    { id: "scripts", label: "Scripts", icon: FileCode2 },
    { id: "templates", label: "Templates", icon: LayoutTemplate },
    { id: "wasm", label: "WASM", icon: Package },
  ];

  async function openScript(entry: GraphemeScriptEntry) {
    if (shouldSuppressScriptContextMenuClick()) return;
    if (embedded) {
      await lmeWorkspace.openScriptById(entry.id);
      return;
    }
    await graphemeScriptEditor.openScriptById(entry.id);
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
    if (embedded) {
      lmeWorkspace.openNewScript();
      return;
    }
    graphemeScriptEditor.openNewTab();
  }

  function startFromRecipe(recipe: GraphemeRecipe) {
    if (embedded) {
      lmeWorkspace.openNewScript();
    } else {
      graphemeScriptEditor.openNewTab();
    }
    graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
    if (embedded) lmeWorkspace.syncScriptTabFromEditor();
  }
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

    {#if !mobile && leftOpen}
      <aside
        class="scripts-workbench-sidebar flex min-h-0 w-[min(280px,28%)] shrink-0 flex-col border-r border-surface-500/40"
      >
        <div class="flex items-center justify-between gap-2 border-b border-surface-500/35 px-3 py-2">
          <p class="workshop-label">{railItems.find((item) => item.id === railSection)?.label}</p>
          <button
            type="button"
            class="workshop-text-action rounded p-1"
            aria-label="Hide sidebar"
            onclick={() => (leftOpen = false)}
          >
            <PanelLeftClose size={14} strokeWidth={1.75} />
          </button>
        </div>

        {#if railSection !== "wasm"}
          <div class="px-3 py-2">
            <input
              class="input w-full text-xs"
              type="search"
              placeholder={railSection === "scripts"
                ? "Search saved scripts…"
                : "Search templates…"}
              bind:value={search}
            />
          </div>
        {/if}

        <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto">
        {#if workshop.loading && workshop.modules.length === 0}
          <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
        {:else if workshop.error}
          <p class="px-3 py-2 text-sm text-error-400">{workshop.error}</p>
        {:else if railSection === "templates"}
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
        {:else if railSection === "scripts"}
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
      </aside>
    {/if}

    <div
      class="scripts-workbench-center relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden {mobile
        ? 'scripts-workbench-center-mobile'
        : ''}"
    >
      <ScriptWorkbenchTitlebar
        {mobile}
        {leftOpen}
        consoleOpen={mobile ? outputSheetOpen : consoleOpen}
        chatOpen={false}
        hideTabStrip={embedded}
        onShowSidebar={() => (leftOpen = true)}
        onToggleConsole={() => (mobile ? (outputSheetOpen = !outputSheetOpen) : (consoleOpen = !consoleOpen))}
        onToggleChat={() => openTools("chat")}
        onOpenOutput={mobile ? openOutput : undefined}
      />

      <div class="flex min-h-0 flex-1 overflow-hidden">
        <div class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <GraphemeScriptEditorPanel {visible} workbenchMode />
          {#if showMobileEmptyHint}
            <div
              class="scripts-workbench-mobile-empty pointer-events-none absolute inset-x-0 top-8 flex justify-center px-6"
            >
              <p class="rounded-lg border border-surface-500/30 bg-surface-900/90 px-3 py-2 text-center text-[11px] text-surface-400">
                Tap
                <span class="text-surface-200"> + </span>
                for templates, or start typing
              </p>
            </div>
          {/if}
          {#if !mobile && consoleOpen}
            <ScriptWorkbenchConsole onHide={() => (consoleOpen = false)} />
          {/if}
        </div>

        {#if chatOpen && !mobile}
          <ScriptWorkbenchChatPanel
            visible={visible}
            onOpenFullChat={() => layout.navigateDesktop("chat", { bump: true })}
          />
        {/if}
      </div>

      <ScriptWorkbenchStatusBar
        onToggleConsole={() => (mobile ? openOutput() : (consoleOpen = true))}
      />

      {#if mobile}
        <ScriptWorkbenchToolsSheet
          open={toolsSheetOpen}
          {visible}
          initialView={toolsInitialView}
          onOpen={() => (toolsSheetOpen = true)}
          onClose={() => (toolsSheetOpen = false)}
        />
        <ScriptWorkbenchOutputSheet
          open={outputSheetOpen}
          onClose={() => (outputSheetOpen = false)}
        />
      {/if}
    </div>
  </div>
</div>
