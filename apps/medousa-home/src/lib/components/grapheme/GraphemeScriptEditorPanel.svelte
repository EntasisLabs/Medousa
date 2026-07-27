<script lang="ts">
  import { onMount } from "svelte";
  import CodeEditorShell from "$lib/components/code/CodeEditorShell.svelte";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import GraphemeRecipeCards from "$lib/components/grapheme/GraphemeRecipeCards.svelte";
  import GraphemeRunResultCard from "$lib/components/grapheme/GraphemeRunResultCard.svelte";
  import WorkshopJourneyBanner from "$lib/components/workshop/WorkshopJourneyBanner.svelte";
  import {
    getCodeEditorLanguage,
    languageSupportsCompile,
    languageSupportsLsp,
    languageSupportsRun,
  } from "$lib/code/codeEditorLanguageRegistry";
  import { connectGraphemeLspClient } from "$lib/grapheme/lspClient";
  import { promoteScriptToFlow } from "$lib/grapheme/graphemeFlowBridge";
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
  import { formatShortcut } from "$lib/platform";

  interface Props {
    visible: boolean;
    workbenchMode?: boolean;
  }

  let { visible, workbenchMode = false }: Props = $props();

  let lspClient = $state<LSPClient | null>(null);
  let lspError = $state<string | null>(null);
  let codeMirror = $state<CodeMirrorHost | undefined>();
  let flowError = $state<string | null>(null);
  let showAdvancedActions = $state(false);
  let pieceLanded = $state(false);
  let pieceLandTimer: ReturnType<typeof setTimeout> | null = null;

  /** Local derived from $state fields — avoids stale class-field `$derived`. */
  const activeTab = $derived(
    graphemeScriptEditor.activeTabId
      ? (graphemeScriptEditor.tabs.find(
          (tab) => tab.tabId === graphemeScriptEditor.activeTabId,
        ) ?? null)
      : null,
  );
  const activeDocumentUri = $derived(
    activeTab ? graphemeScriptEditor.documentUriForTab(activeTab) : null,
  );
  const activeLanguage = $derived(
    getCodeEditorLanguage(activeTab?.languageId ?? "grapheme"),
  );
  const canUseLsp = $derived(languageSupportsLsp(activeLanguage.id));
  const canCompile = $derived(languageSupportsCompile(activeLanguage.id));
  const canRun = $derived(languageSupportsRun(activeLanguage.id));
  const canSave = $derived(activeLanguage.capabilities.saveToLibrary);
  const canAddToFlow = $derived(activeLanguage.capabilities.addToFlow);

  function flashPieceLanded() {
    pieceLanded = true;
    if (pieceLandTimer) clearTimeout(pieceLandTimer);
    pieceLandTimer = setTimeout(() => {
      pieceLanded = false;
      pieceLandTimer = null;
    }, 480);
  }

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
      flashPieceLanded();
      return;
    }
    graphemeScriptEditor.appendToActiveBody(pending);
    graphemeScriptEditor.clearPendingInsert();
    flashPieceLanded();
  });

  const showRecipePicker = $derived(
    Boolean(canCompile && activeTab && !activeTab.body.trim()),
  );

  function startFromRecipe(recipe: GraphemeRecipe) {
    graphemeScriptEditor.ensureInitialTab();
    graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
    graphemeScriptEditor.sidePane = "diagnostics";
    flowError = null;
  }

  function addActiveScriptToFlow() {
    flowError = null;
    const tab = activeTab;
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
    const tab = activeTab;
    if (!tab || !canSave) return;
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
    const tab = activeTab;
    if (!tab?.body.trim() || !canCompile) return;
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
    const tab = activeTab;
    if (!tab?.body.trim() || !canRun) return;
    graphemeScriptEditor.sidePane = "diagnostics";
    await workshop.runScriptSource(tab.body);
    graphemeScriptEditor.runError = workshop.runError;
  }
</script>

<CodeEditorShell {workbenchMode} {pieceLanded}>
  {#snippet header()}
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="text-sm font-semibold text-surface-50">Script editor</p>
        <p class="workshop-header-line mt-0.5">
          {activeLanguage.label}
          {#if canRun}
            · run, save, add to flow
          {:else if activeLanguage.tier === "highlight"}
            · highlight only
          {:else if activeLanguage.tier === "stub"}
            · preview stub
          {/if}
        </p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        {#if canAddToFlow}
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={!activeTab?.body.trim()}
            onclick={addActiveScriptToFlow}
          >
            Add to flow
          </button>
        {/if}
        {#if canSave}
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            disabled={graphemeScriptEditor.saveBusy || !activeTab}
            onclick={() => void saveActive()}
          >
            {graphemeScriptEditor.saveBusy ? "Saving…" : "Save"}
          </button>
        {/if}
        {#if canRun}
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={workshop.runBusy || !activeTab?.body.trim()}
            onclick={() => void runActive()}
          >
            {workshop.runBusy ? "Running…" : "Run"}
          </button>
        {/if}
        {#if canCompile}
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={graphemeScriptEditor.compileBusy || !activeTab?.body.trim()}
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
              disabled={graphemeScriptEditor.compileBusy || !activeTab?.body.trim()}
              onclick={() => void compileActive("aot")}
            >
              Optimize (AOT)
            </button>
          {/if}
        {/if}
      </div>
    </div>
  {/snippet}

  {#snippet toolbar()}
    {#if flowError}
      <p class="mt-2 text-xs text-error-400">{flowError}</p>
    {/if}

    <div class="mt-3 px-1">
      <WorkshopJourneyBanner compact />
    </div>
  {/snippet}

  {#snippet beforeEditor()}
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
  {/snippet}

  {#snippet editor()}
    {#if activeTab}
      {#key `${activeTab.tabId}:${graphemeScriptEditor.contentEpoch}:${activeLanguage.id}:${lspClient && canUseLsp ? "lsp" : "plain"}`}
        <CodeMirrorHost
          bind:this={codeMirror}
          value={activeTab.body}
          languageId={activeLanguage.id}
          documentUri={activeDocumentUri}
          client={canUseLsp ? lspClient : null}
          contentSyncKey={graphemeScriptEditor.contentEpoch}
          onchange={(body) => graphemeScriptEditor.patchActiveTab({ body })}
        />
      {/key}
    {:else}
      <p class="workshop-muted p-4 text-sm">Open or create a script tab.</p>
    {/if}
  {/snippet}

  {#snippet sidePane()}
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
        class="rounded-md px-2 py-1 text-[11px] {graphemeScriptEditor.sidePane === 'diagnostics'
          ? 'bg-surface-800 text-primary-300'
          : 'text-surface-400'}"
        onclick={() => (graphemeScriptEditor.sidePane = "diagnostics")}
      >
        Diagnostics
      </button>
    </div>

    {#if graphemeScriptEditor.sidePane === "info"}
      {#if activeTab}
        <label class="mt-4 block">
          <span class="workshop-label">Name</span>
          <input
            class="input mt-1 w-full text-sm"
            value={activeTab.name}
            oninput={(event) =>
              graphemeScriptEditor.patchActiveTab({
                name: (event.currentTarget as HTMLInputElement).value,
              })}
          />
        </label>
        <div class="mt-3 block">
          <span class="workshop-label">Language</span>
          <p class="workshop-muted mt-1 text-sm">{activeLanguage.label}</p>
        </div>
        {#if canSave}
          <label class="mt-3 block">
            <span class="workshop-label">Intent</span>
            <input
              class="input mt-1 w-full text-sm"
              value={activeTab.intent}
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
              value={activeTab.tags.join(", ")}
              oninput={(event) =>
                graphemeScriptEditor.patchActiveTab({
                  tags: (event.currentTarget as HTMLInputElement).value
                    .split(",")
                    .map((tag) => tag.trim())
                    .filter(Boolean),
                })}
            />
          </label>
          {#if activeTab.scriptId}
            <p class="workshop-faint mt-3 font-mono text-[11px]">
              {activeTab.scriptId} · v{activeTab.version}
            </p>
          {/if}
        {:else}
          <p class="workshop-muted mt-3 text-xs">
            Snippet tabs are local to the workbench — not saved to the script library.
          </p>
        {/if}
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
    {:else if canUseLsp && lspError}
      <p class="mt-4 text-sm text-warning-400">Smart editing unavailable: {lspError}</p>
    {:else if canUseLsp && graphemeScriptEditor.lspReady}
      <p class="workshop-muted mt-4 text-sm">
        Tips and completions appear as you type.
      </p>
    {:else if canUseLsp}
      <p class="workshop-muted mt-4 text-sm">Loading editor helpers…</p>
    {:else}
      <p class="workshop-muted mt-4 text-sm">
        {activeLanguage.tier === "stub"
          ? `${activeLanguage.label} syntax plug-in is not wired yet — editing as plain text.`
          : `${activeLanguage.label} — syntax highlighting only.`}
      </p>
    {/if}

    {#if canRun && (workshop.runResult || workshop.runError || (!graphemeScriptEditor.compileResult && !graphemeScriptEditor.compileError))}
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
  {/snippet}

  {#snippet statusBar()}
    <div class="flex flex-wrap items-center justify-between gap-2 text-[11px]">
      <span class="text-surface-400">
        {#if activeTab}
          {activeTab.dirty ? "Modified · " : ""}
          {activeTab.body.split("\n").length} lines
          · {activeLanguage.label}
          {#if canUseLsp && graphemeScriptEditor.lspReady}
            · completions on
          {/if}
        {:else}
          No active script
        {/if}
      </span>
      <span class="text-surface-500">
        {#if canSave}
          <kbd class="vault-kbd">{formatShortcut("S")}</kbd> save ·
        {/if}
        {#if canUseLsp}
          <kbd class="vault-kbd">F12</kbd> go to definition
        {/if}
      </span>
    </div>
  {/snippet}
</CodeEditorShell>
