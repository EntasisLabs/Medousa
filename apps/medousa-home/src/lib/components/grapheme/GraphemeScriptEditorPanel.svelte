<script lang="ts">
  import { onMount } from "svelte";
  import ScriptEditorTabStrip from "$lib/components/automations/ScriptEditorTabStrip.svelte";
  import GraphemeCodeMirror from "$lib/components/grapheme/GraphemeCodeMirror.svelte";
  import GraphemeRecipeCards from "$lib/components/grapheme/GraphemeRecipeCards.svelte";
  import GraphemeRunResultCard from "$lib/components/grapheme/GraphemeRunResultCard.svelte";
  import WorkshopJourneyBanner from "$lib/components/workshop/WorkshopJourneyBanner.svelte";
  import { connectGraphemeLspClient } from "$lib/grapheme/lspClient";
  import { promoteScriptToFlow } from "$lib/grapheme/graphemeFlowBridge";
  import { prepareModuleInsert } from "$lib/grapheme/graphemeModuleSnippet";
  import {
    applyRecipeToEditor,
    type GraphemeRecipe,
  } from "$lib/grapheme/graphemeRecipes";
  import {
    compileGraphemeSource,
    getGraphemeLspWorkspace,
    saveGraphemeScript,
  } from "$lib/daemon";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { LSPClient } from "@codemirror/lsp-client";

  interface Props {
    visible: boolean;
    workbenchMode?: boolean;
  }

  let { visible, workbenchMode = false }: Props = $props();

  let lspClient = $state<LSPClient | null>(null);
  let lspError = $state<string | null>(null);
  let codeMirror = $state<GraphemeCodeMirror | undefined>();
  let modulePickerId = $state("");
  let flowError = $state<string | null>(null);
  let showAdvancedActions = $state(false);

  onMount(() => {
    void getGraphemeLspWorkspace().then((workspace) => {
      graphemeScriptEditor.lspWorkspace = workspace;
    });
    void connectGraphemeLspClient()
      .then(({ client, workspace }) => {
        lspClient = client;
        graphemeScriptEditor.lspWorkspace = workspace;
        graphemeScriptEditor.lspReady = true;
      })
      .catch((err) => {
        lspError = err instanceof Error ? err.message : String(err);
      });
  });

  $effect(() => {
    if (!visible) return;
    if (workshop.modules.length === 0) {
      void workshop.refreshModulesAndScripts();
    }
  });

  $effect(() => {
    const pending = graphemeScriptEditor.pendingInsert;
    if (!pending) return;
    if (codeMirror) {
      codeMirror.insertText(pending);
      codeMirror.focusEditor();
      graphemeScriptEditor.clearPendingInsert();
      return;
    }
    graphemeScriptEditor.appendToActiveBody(pending);
    graphemeScriptEditor.clearPendingInsert();
  });

  const showRecipePicker = $derived(
    Boolean(
      graphemeScriptEditor.activeTab &&
        !graphemeScriptEditor.activeTab.body.trim(),
    ),
  );

  const moduleDetail = $derived(workshop.moduleDetail);
  const moduleOps = $derived(moduleDetail?.info.exported_ops ?? []);
  const moduleExamples = $derived(moduleDetail?.examples ?? []);

  $effect(() => {
    if (graphemeScriptEditor.modulesPaneModuleId) {
      modulePickerId = graphemeScriptEditor.modulesPaneModuleId;
      graphemeScriptEditor.modulesPaneModuleId = null;
    }
  });

  $effect(() => {
    if (graphemeScriptEditor.sidePane !== "modules") return;
    if (!modulePickerId && workshop.modules.length > 0) {
      modulePickerId = workshop.modules[0]?.module_id ?? "";
    }
    if (modulePickerId) {
      void workshop.loadModuleDetail(modulePickerId);
    }
  });

  function insertModuleOp(op: string) {
    const body = graphemeScriptEditor.activeTab?.body ?? "";
    graphemeScriptEditor.queueInsert(
      prepareModuleInsert(body, op, moduleExamples),
    );
  }

  function startFromRecipe(recipe: GraphemeRecipe) {
    graphemeScriptEditor.ensureInitialTab();
    graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
    graphemeScriptEditor.sidePane = "diagnostics";
    flowError = null;
  }

  function addActiveScriptToFlow() {
    flowError = null;
    const tab = graphemeScriptEditor.activeTab;
    if (!tab?.body.trim()) {
      flowError = "Write script source before adding to a flow.";
      return;
    }
    try {
      promoteScriptToFlow(tab.body, tab.name, tab.scriptId);
    } catch (err) {
      flowError = err instanceof Error ? err.message : String(err);
    }
  }

  async function saveActive() {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab) return;
    graphemeScriptEditor.saveBusy = true;
    graphemeScriptEditor.saveError = null;
    try {
      const response = await saveGraphemeScript({
        id: tab.scriptId,
        name: tab.name.trim() || "Untitled script",
        body: tab.body,
        intent: tab.intent.trim() || null,
        tags: tab.tags,
      });
      graphemeScriptEditor.markActiveSaved(response.script);
      await workshop.refreshModulesAndScripts();
    } catch (err) {
      graphemeScriptEditor.saveError =
        err instanceof Error ? err.message : String(err);
    } finally {
      graphemeScriptEditor.saveBusy = false;
    }
  }

  async function compileActive(mode: "check" | "aot") {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab?.body.trim()) return;
    graphemeScriptEditor.compileBusy = true;
    graphemeScriptEditor.compileError = null;
    graphemeScriptEditor.compileResult = null;
    try {
      graphemeScriptEditor.compileResult = await compileGraphemeSource(
        tab.body,
        mode,
      );
      graphemeScriptEditor.sidePane = "diagnostics";
    } catch (err) {
      graphemeScriptEditor.compileError =
        err instanceof Error ? err.message : String(err);
    } finally {
      graphemeScriptEditor.compileBusy = false;
    }
  }

  async function runActive() {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab?.body.trim()) return;
    graphemeScriptEditor.sidePane = "diagnostics";
    await workshop.runScriptSource(tab.body);
    graphemeScriptEditor.runError = workshop.runError;
  }
</script>

<div class="grapheme-script-editor flex min-h-0 flex-1 flex-col overflow-hidden">
  {#if !workbenchMode}
  <header class="workshop-header shrink-0 border-b border-surface-500/40 px-4 py-3">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="text-sm font-semibold text-surface-50">Script editor</p>
        <p class="workshop-header-line mt-0.5">Grapheme · run, save, add to flow</p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="btn btn-sm variant-soft-surface"
          disabled={!graphemeScriptEditor.activeTab?.body.trim()}
          onclick={addActiveScriptToFlow}
        >
          Add to flow
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={graphemeScriptEditor.saveBusy || !graphemeScriptEditor.activeTab}
          onclick={() => void saveActive()}
        >
          {graphemeScriptEditor.saveBusy ? "Saving…" : "Save"}
        </button>
        <button
          type="button"
          class="btn btn-sm variant-soft-surface"
          disabled={workshop.runBusy || !graphemeScriptEditor.activeTab?.body.trim()}
          onclick={() => void runActive()}
        >
          {workshop.runBusy ? "Running…" : "Run"}
        </button>
        <button
          type="button"
          class="btn btn-sm variant-soft-surface"
          disabled={graphemeScriptEditor.compileBusy || !graphemeScriptEditor.activeTab?.body.trim()}
          onclick={() => void compileActive("check")}
        >
          Compile
        </button>
        <button
          type="button"
          class="workshop-text-action btn btn-sm variant-ghost-surface text-[11px]"
          onclick={() => (showAdvancedActions = !showAdvancedActions)}
        >
          {showAdvancedActions ? "Less" : "Advanced"}
        </button>
        {#if showAdvancedActions}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={graphemeScriptEditor.compileBusy || !graphemeScriptEditor.activeTab?.body.trim()}
            onclick={() => void compileActive("aot")}
          >
            Optimize (AOT)
          </button>
        {/if}
      </div>
    </div>

    {#if flowError}
      <p class="mt-2 text-xs text-error-400">{flowError}</p>
    {/if}

    <div class="mt-3 px-1">
      <WorkshopJourneyBanner compact />
    </div>

    <div class="mt-3 border-b border-surface-600/50 pb-px">
      <ScriptEditorTabStrip />
    </div>
  </header>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {#if showRecipePicker && settings.showWorkshopGuidance && !workbenchMode}
        <div class="shrink-0 border-b border-surface-500/35 px-4 py-3">
          <GraphemeRecipeCards
            compact
            title="Starter recipes"
            hint="Optional — or type in the editor below."
            onselect={startFromRecipe}
          />
        </div>
      {/if}
      {#if graphemeScriptEditor.activeTab && graphemeScriptEditor.activeDocumentUri}
        {#key `${graphemeScriptEditor.activeTab.tabId}:${graphemeScriptEditor.activeDocumentUri}:${lspClient ? "lsp" : "plain"}`}
          <GraphemeCodeMirror
            bind:this={codeMirror}
            value={graphemeScriptEditor.activeTab.body}
            documentUri={graphemeScriptEditor.activeDocumentUri}
            client={lspClient}
            onchange={(body) => graphemeScriptEditor.patchActiveTab({ body })}
          />
        {/key}
      {:else}
        <p class="workshop-muted p-4 text-sm">Open or create a script tab.</p>
      {/if}
    </div>

    {#if !workbenchMode}
    <aside class="grapheme-script-side-pane w-[min(360px,34%)] shrink-0 overflow-y-auto border-l border-surface-500/40 px-4 py-4">
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          class="rounded-md px-2 py-1 text-[11px] {graphemeScriptEditor.sidePane === 'info'
            ? 'bg-surface-800 text-primary-300'
            : 'text-surface-400'}"
          onclick={() => (graphemeScriptEditor.sidePane = "info")}
        >
          Info
        </button>
        <button
          type="button"
          class="rounded-md px-2 py-1 text-[11px] {graphemeScriptEditor.sidePane === 'modules'
            ? 'bg-surface-800 text-primary-300'
            : 'text-surface-400'}"
          onclick={() => (graphemeScriptEditor.sidePane = "modules")}
        >
          Modules
        </button>
        <button
          type="button"
          class="rounded-md px-2 py-1 text-[11px] {graphemeScriptEditor.sidePane === 'diagnostics'
            ? 'bg-surface-800 text-primary-300'
            : 'text-surface-400'}"
          onclick={() => (graphemeScriptEditor.sidePane = "diagnostics")}
        >
          Diagnostics
        </button>
      </div>

      {#if graphemeScriptEditor.sidePane === "info"}
        {#if graphemeScriptEditor.activeTab}
          <label class="mt-4 block">
            <span class="workshop-label">Name</span>
            <input
              class="input mt-1 w-full text-sm"
              value={graphemeScriptEditor.activeTab.name}
              oninput={(event) =>
                graphemeScriptEditor.patchActiveTab({
                  name: (event.currentTarget as HTMLInputElement).value,
                })}
            />
          </label>
          <label class="mt-3 block">
            <span class="workshop-label">Intent</span>
            <input
              class="input mt-1 w-full text-sm"
              value={graphemeScriptEditor.activeTab.intent}
              oninput={(event) =>
                graphemeScriptEditor.patchActiveTab({
                  intent: (event.currentTarget as HTMLInputElement).value,
                })}
            />
          </label>
          <label class="mt-3 block">
            <span class="workshop-label">Tags</span>
            <input
              class="input mt-1 w-full text-sm"
              value={graphemeScriptEditor.activeTab.tags.join(", ")}
              oninput={(event) =>
                graphemeScriptEditor.patchActiveTab({
                  tags: (event.currentTarget as HTMLInputElement).value
                    .split(",")
                    .map((tag) => tag.trim())
                    .filter(Boolean),
                })}
            />
          </label>
          {#if graphemeScriptEditor.activeTab.scriptId}
            <p class="workshop-faint mt-3 font-mono text-[11px]">
              {graphemeScriptEditor.activeTab.scriptId} · v{graphemeScriptEditor.activeTab.version}
            </p>
          {/if}
        {/if}
      {:else if graphemeScriptEditor.sidePane === "modules"}
        <label class="mt-4 block">
          <span class="workshop-label">Module</span>
          <select
            class="input mt-1 w-full text-sm"
            bind:value={modulePickerId}
            onchange={() => {
              if (modulePickerId) void workshop.loadModuleDetail(modulePickerId);
            }}
          >
            {#each workshop.modules as entry (entry.module_id)}
              <option value={entry.module_id}>{entry.module_id}</option>
            {/each}
          </select>
        </label>

        {#if workshop.moduleDetailLoading}
          <p class="workshop-muted mt-4 text-sm">Loading module ops…</p>
        {:else if workshop.moduleDetailError}
          <p class="mt-4 text-sm text-warning-400">{workshop.moduleDetailError}</p>
        {:else if moduleOps.length === 0}
          <p class="workshop-muted mt-4 text-sm">No exported ops for this module.</p>
        {:else}
          <ul class="mt-4 space-y-2">
            {#each moduleOps as op (op.op)}
              <li class="rounded-md border border-surface-500/35 px-3 py-2 text-xs">
                <div class="flex items-start justify-between gap-2">
                  <div class="min-w-0">
                    <p class="font-mono text-surface-100">{op.op}</p>
                    <p class="workshop-faint mt-1">{op.output_type}</p>
                  </div>
                  <button
                    type="button"
                    class="workshop-text-action shrink-0 text-[11px]"
                    onclick={() => insertModuleOp(op.op)}
                  >
                    Insert
                  </button>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if graphemeScriptEditor.compileError}
        <p class="mt-4 text-sm text-error-400">{graphemeScriptEditor.compileError}</p>
      {:else if graphemeScriptEditor.compileResult}
        <div class="mt-4 space-y-2 text-xs">
          <p class="font-medium text-surface-100">
            {graphemeScriptEditor.compileResult.mode} ·
            {graphemeScriptEditor.compileResult.validated ? "valid" : "invalid"}
          </p>
          {#each graphemeScriptEditor.compileResult.compile_hints as hint (hint)}
            <p class="text-surface-300">{hint}</p>
          {/each}
          {#each graphemeScriptEditor.compileResult.lint_warnings as warning (warning)}
            <p class="text-warning-400">{warning}</p>
          {/each}
        </div>
      {:else if lspError}
        <p class="mt-4 text-sm text-warning-400">Smart editing unavailable: {lspError}</p>
      {:else if graphemeScriptEditor.lspReady}
        <p class="workshop-muted mt-4 text-sm">
          Tips and completions appear as you type.
        </p>
      {:else}
        <p class="workshop-muted mt-4 text-sm">Loading editor helpers…</p>
      {/if}

      {#if workshop.runResult || workshop.runError || (!graphemeScriptEditor.compileResult && !graphemeScriptEditor.compileError)}
        <GraphemeRunResultCard
          result={workshop.runResult?.result}
          error={workshop.runError}
          emptyMessage={showRecipePicker
            ? "Pick a recipe, then hit Try it."
            : "Hit Try it to see results here."}
        />
      {/if}
      {#if graphemeScriptEditor.saveError}
        <p class="mt-4 text-xs text-error-400">{graphemeScriptEditor.saveError}</p>
      {/if}
    </aside>
    {/if}
  </div>

  {#if !workbenchMode}
  <footer class="workshop-status shrink-0 border-t border-surface-500/40 px-4 py-2">
    <div class="flex flex-wrap items-center justify-between gap-2 text-[11px]">
      <span class="text-surface-400">
        {#if graphemeScriptEditor.activeTab}
          {graphemeScriptEditor.activeTab.dirty ? "Modified · " : ""}
          {graphemeScriptEditor.activeTab.body.split("\n").length} lines
          {#if graphemeScriptEditor.lspReady}
            · completions on
          {/if}
        {:else}
          No active script
        {/if}
      </span>
      <span class="text-surface-500">
        <kbd class="vault-kbd">⌘S</kbd> save ·
        <kbd class="vault-kbd">F12</kbd> go to definition
      </span>
    </div>
  </footer>
  {/if}
</div>
