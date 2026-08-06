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
  import ScriptWorkbenchTitlebar from "$lib/components/automations/ScriptWorkbenchTitlebar.svelte";
  import ScriptWorkbenchToolsSheet from "$lib/components/automations/ScriptWorkbenchToolsSheet.svelte";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import GraphemeScriptEditorPanel from "$lib/components/grapheme/GraphemeScriptEditorPanel.svelte";
  import { applyRecipeToEditor, GRAPHEME_STARTER_RECIPES, type GraphemeRecipe } from "$lib/grapheme/graphemeRecipes";
  import { renameScriptById } from "$lib/grapheme/scriptWorkbenchActions";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { scriptLibrarySelection } from "$lib/stores/scriptLibrarySelection.svelte";
  import { scriptRenameUi } from "$lib/stores/scriptRenameUi.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeScriptEntry } from "$lib/types/grapheme";
  import {
    bindScriptLongPress,
    handleScriptContextMenuEvent,
    shouldSuppressScriptContextMenuClick,
  } from "$lib/utils/scriptContextMenuEvents";
  import { SCRIPT_WORKBENCH_OPEN_CONSOLE_EVENT } from "$lib/utils/scriptWorkbenchChromeEvents";
  import { onMount, tick } from "svelte";

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

  /** Mobile More uses embedded=true but never mounts LME — keep opens on grapheme editor. */
  const useLmeScriptChrome = $derived(embedded && !mobile);

  const mobileActiveTab = $derived(
    graphemeScriptEditor.activeTabId
      ? (graphemeScriptEditor.tabs.find(
          (tab) => tab.tabId === graphemeScriptEditor.activeTabId,
        ) ?? null)
      : null,
  );

  const showMobileEmptyHint = $derived(
    mobile && Boolean(mobileActiveTab && !mobileActiveTab.body.trim()),
  );

  function openTools(view: typeof toolsInitialView = "root") {
    toolsInitialView = view;
    toolsSheetOpen = true;
  }

  function openOutput() {
    outputSheetOpen = true;
  }

  onMount(() => {
    graphemeScriptEditor.ensureInitialTab();
  });

  $effect(() => {
    if (!visible) return;
    void workshop.refreshModulesAndScripts();
    graphemeScriptEditor.ensureInitialTab();
  });

  $effect(() => {
    if (!mobile || !visible) return;
    automationsNav.setMobileChromeMode("script-editor");
    return () => {
      if (automationsNav.mobileChromeMode === "script-editor") {
        automationsNav.setMobileChromeMode("browse");
      }
    };
  });

  $effect(() => {
    if (mobile || !visible) return;
    const onOpen = () => {
      consoleOpen = true;
    };
    window.addEventListener(SCRIPT_WORKBENCH_OPEN_CONSOLE_EVENT, onOpen);
    return () => window.removeEventListener(SCRIPT_WORKBENCH_OPEN_CONSOLE_EVENT, onOpen);
  });

  $effect(() => {
    if (!mobile || !visible) return;
    const onTools = () => openTools("root");
    const onSearch = () => openTools("library");
    window.addEventListener("medousa-mobile-automations-tools", onTools);
    window.addEventListener("medousa-mobile-automations-search-focus", onSearch);
    return () => {
      window.removeEventListener("medousa-mobile-automations-tools", onTools);
      window.removeEventListener("medousa-mobile-automations-search-focus", onSearch);
    };
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
  const filteredScriptIds = $derived(filteredScripts.map((entry) => entry.id));

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
    graphemeScriptEditor.ensureInitialTab();
    const current = mobileActiveTab ?? graphemeScriptEditor.activeTab;
    if (current?.body.trim()) {
      if (useLmeScriptChrome) lmeWorkspace.openNewScript();
      else graphemeScriptEditor.openNewTab();
    }
    graphemeScriptEditor.loadExternalContent(applyRecipeToEditor(recipe));
    if (useLmeScriptChrome) lmeWorkspace.syncScriptTabFromEditor({ activate: true });
  }

  const railItems: { id: RailSection; label: string; icon: typeof FileCode2 }[] = [
    { id: "scripts", label: "Scripts", icon: FileCode2 },
    { id: "templates", label: "Templates", icon: LayoutTemplate },
    { id: "wasm", label: "WASM", icon: Package },
  ];

  async function openScript(entry: GraphemeScriptEntry, event?: MouseEvent) {
    if (shouldSuppressScriptContextMenuClick()) return;
    if (!scriptLibrarySelection.applySelection(entry.id, event, filteredScriptIds)) {
      return;
    }
    if (useLmeScriptChrome) {
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
    if (useLmeScriptChrome) {
      lmeWorkspace.openNewScript();
      return;
    }
    graphemeScriptEditor.openNewTab();
  }

  function startFromRecipe(recipe: GraphemeRecipe) {
    if (useLmeScriptChrome) {
      lmeWorkspace.openNewScript();
    } else {
      graphemeScriptEditor.openNewTab();
    }
    graphemeScriptEditor.loadExternalContent(applyRecipeToEditor(recipe));
    if (useLmeScriptChrome) lmeWorkspace.syncScriptTabFromEditor({ activate: true });
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
          <p class="px-3 py-2 text-sm text-content-error">{workshop.error}</p>
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
                      class="flex w-full flex-col px-3 py-2 text-left transition hover:bg-surface-800/70 {scriptLibrarySelection.isSelected(
                        entry.id,
                      ) || graphemeScriptEditor.activeTab?.scriptId === entry.id
                        ? 'workshop-list-row-active'
                        : ''}"
                      onclick={(event) => void openScript(entry, event)}
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
              <p class="text-xs text-content-error">{workshop.moduleLoadError}</p>
            {:else if workshop.moduleLoadResult}
              <p class="text-xs text-content-secondary">
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
                <p class="mt-2 text-xs text-content-error">{workshop.lifecycleError}</p>
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

      {#if mobile && mobileActiveTab}
        <div
          class="scripts-workbench-mobile-title shrink-0 border-b border-surface-500/30 px-3 py-2"
        >
          <p class="truncate text-sm font-medium text-surface-100">
            {mobileActiveTab.name}
          </p>
          {#if mobileActiveTab.body.trim()}
            <p class="workshop-faint mt-0.5 truncate font-mono text-[10px] leading-snug">
              {mobileActiveTab.body.trim().split("\n")[0]}
            </p>
          {:else}
            <p class="workshop-faint mt-0.5 text-[10px]">Empty script</p>
          {/if}
        </div>
      {/if}

      <div class="flex min-h-0 flex-1 overflow-hidden">
        <div class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          {#if mobile}
            {#if mobileActiveTab}
              {#key `${mobileActiveTab.tabId}:${graphemeScriptEditor.contentEpoch}`}
                <CodeMirrorHost
                  value={mobileActiveTab.body}
                  languageId="grapheme"
                  contentSyncKey={graphemeScriptEditor.contentEpoch}
                  onchange={(body) => {
                    const id = graphemeScriptEditor.activeTabId;
                    if (id) graphemeScriptEditor.patchTab(id, { body });
                  }}
                />
              {/key}
            {:else}
              <div class="flex flex-1 flex-col items-center justify-center gap-3 px-6">
                <p class="workshop-muted text-center text-sm">No script tab yet.</p>
                <button
                  type="button"
                  class="btn btn-sm variant-filled-primary"
                  onclick={() => graphemeScriptEditor.openNewTab()}
                >
                  New script
                </button>
              </div>
            {/if}
          {:else}
            <GraphemeScriptEditorPanel {visible} workbenchMode />
          {/if}
          {#if showMobileEmptyHint}
            <div
              class="scripts-workbench-mobile-empty pointer-events-none absolute inset-x-0 top-8 flex justify-center px-6"
            >
              <p class="rounded-lg border border-surface-500/30 bg-surface-900/90 px-3 py-2 text-center text-[11px] text-content-tertiary">
                Open Script tools in the top bar for templates, or start typing
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

      {#if mobile}
        <ScriptWorkbenchToolsSheet
          open={toolsSheetOpen}
          {visible}
          initialView={toolsInitialView}
          hideFab={true}
          onOpen={() => (toolsSheetOpen = true)}
          onClose={() => (toolsSheetOpen = false)}
          onApplyTemplate={applyTemplate}
          onOpenScript={openScript}
          onNewScript={startNewScript}
        />
        <ScriptWorkbenchOutputSheet
          open={outputSheetOpen}
          onClose={() => (outputSheetOpen = false)}
        />
      {/if}
    </div>
  </div>
</div>
