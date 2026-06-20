<script lang="ts">
  import { onMount } from "svelte";
  import { Plus, X } from "@lucide/svelte";
  import GraphemeCodeMirror from "$lib/components/grapheme/GraphemeCodeMirror.svelte";
  import { connectGraphemeLspClient } from "$lib/grapheme/lspClient";
  import {
    compileGraphemeSource,
    getGraphemeLspWorkspace,
    saveGraphemeScript,
  } from "$lib/daemon";
  import { formatGraphemeRunResult } from "$lib/grapheme/graphemeRunOutput";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { LSPClient } from "@codemirror/lsp-client";

  interface Props {
    visible: boolean;
  }

  let { visible }: Props = $props();

  let lspClient = $state<LSPClient | null>(null);
  let lspError = $state<string | null>(null);

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

  const runOutputText = $derived(
    workshop.runError
      ? null
      : formatGraphemeRunResult(workshop.runResult?.result),
  );

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
  <header class="workshop-header shrink-0 border-b border-surface-500/40 px-4 py-3">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="text-sm font-semibold text-surface-50">Script workshop</p>
        <p class="workshop-header-line mt-0.5">
          Grapheme scripts with LSP completion, diagnostics, and run/save policy
        </p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
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
          disabled={graphemeScriptEditor.compileBusy || !graphemeScriptEditor.activeTab?.body.trim()}
          onclick={() => void compileActive("check")}
        >
          Check
        </button>
        <button
          type="button"
          class="btn btn-sm variant-soft-surface"
          disabled={graphemeScriptEditor.compileBusy || !graphemeScriptEditor.activeTab?.body.trim()}
          onclick={() => void compileActive("aot")}
        >
          AOT
        </button>
        <button
          type="button"
          class="btn btn-sm variant-soft-surface"
          disabled={workshop.runBusy || !graphemeScriptEditor.activeTab?.body.trim()}
          onclick={() => void runActive()}
        >
          {workshop.runBusy ? "Running…" : "Run"}
        </button>
      </div>
    </div>

    <div class="mt-3 flex items-center gap-1 overflow-x-auto border-b border-surface-600/50 pb-px">
      {#each graphemeScriptEditor.tabs as tab (tab.tabId)}
        <div
          class="group flex max-w-[220px] items-center gap-1 rounded-t-md border border-b-0 px-2 py-1 text-[11px] {graphemeScriptEditor.activeTabId ===
          tab.tabId
            ? 'border-surface-500/60 bg-surface-900 text-primary-300'
            : 'border-transparent bg-transparent text-surface-400 hover:bg-surface-800/70'}"
        >
          <button
            type="button"
            class="min-w-0 truncate"
            onclick={() => graphemeScriptEditor.selectTab(tab.tabId)}
          >
            {tab.dirty ? "*" : ""}{tab.name}
          </button>
          {#if graphemeScriptEditor.tabs.length > 1}
            <button
              type="button"
              class="rounded p-0.5 text-surface-500 opacity-0 transition group-hover:opacity-100 hover:text-surface-200"
              aria-label="Close tab"
              onclick={() => graphemeScriptEditor.closeTab(tab.tabId)}
            >
              <X size={12} strokeWidth={2} />
            </button>
          {/if}
        </div>
      {/each}
      <button
        type="button"
        class="rounded-md p-1 text-surface-400 hover:bg-surface-800 hover:text-surface-100"
        aria-label="New script tab"
        onclick={() => graphemeScriptEditor.openNewTab()}
      >
        <Plus size={14} strokeWidth={2} />
      </button>
    </div>
  </header>

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {#if graphemeScriptEditor.activeTab && graphemeScriptEditor.activeDocumentUri}
        {#key `${graphemeScriptEditor.activeTab.tabId}:${graphemeScriptEditor.activeDocumentUri}:${lspClient ? "lsp" : "plain"}`}
          <GraphemeCodeMirror
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

    <aside class="grapheme-script-side-pane w-[min(360px,34%)] shrink-0 overflow-y-auto border-l border-surface-500/40 px-4 py-4">
      <div class="flex gap-2">
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
        <p class="mt-4 text-sm text-warning-400">LSP: {lspError}</p>
      {:else if graphemeScriptEditor.lspReady}
        <p class="workshop-muted mt-4 text-sm">
          LSP connected — parse errors and completions appear inline.
        </p>
      {:else}
        <p class="workshop-muted mt-4 text-sm">Connecting to Grapheme LSP…</p>
      {/if}

      {#if workshop.runError}
        <p class="mt-4 text-xs text-error-400">{workshop.runError}</p>
      {:else if runOutputText}
        <pre class="grapheme-run-output mt-4 max-h-72 overflow-auto rounded-md border border-surface-500/35 p-3 font-mono text-[11px] leading-relaxed text-surface-200 whitespace-pre-wrap">{runOutputText}</pre>
      {/if}
      {#if graphemeScriptEditor.saveError}
        <p class="mt-4 text-xs text-error-400">{graphemeScriptEditor.saveError}</p>
      {/if}
    </aside>
  </div>

  <footer class="workshop-status shrink-0 border-t border-surface-500/40 px-4 py-2">
    <div class="flex flex-wrap items-center justify-between gap-2 text-[11px]">
      <span class="text-surface-400">
        {#if graphemeScriptEditor.activeTab}
          {graphemeScriptEditor.activeTab.dirty ? "Modified · " : ""}
          {graphemeScriptEditor.activeTab.body.split("\n").length} lines
          {#if graphemeScriptEditor.lspReady}
            · LSP on
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
</div>
