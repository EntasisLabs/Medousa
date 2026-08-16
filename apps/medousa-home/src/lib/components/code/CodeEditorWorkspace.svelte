<script lang="ts">
  import { LoaderCircle, Sparkles } from "@lucide/svelte";
  import type { LSPClient } from "@codemirror/lsp-client";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import CodeWorkspaceSearch from "$lib/components/code/CodeWorkspaceSearch.svelte";
  import CodeChangesPanel from "$lib/components/code/CodeChangesPanel.svelte";
  import CodeContextSidePanel from "$lib/components/code/CodeContextSidePanel.svelte";
  import CodeTasksOutput from "$lib/components/code/CodeTasksOutput.svelte";
  import CodeTerminalDock from "$lib/components/work/CodeTerminalDock.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import type { CodeChangesController } from "$lib/code/codeChangesController.svelte";
  import type { CodeProblemsController } from "$lib/code/codeProblemsController.svelte";
  import type { CodeQuickOpenController } from "$lib/code/codeQuickOpenController.svelte";
  import type { CodeSaveController } from "$lib/code/codeSaveController.svelte";
  import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
  import { codeEditorLspLanguageId } from "$lib/code/codeEditorLanguageRegistry";
  import {
    humanizeForgeMessage,
    type ForgeSourceFile,
  } from "$lib/code/codeDocumentService";
  import type { CodeDocumentSymbol, CodeLanguageMatrixEntry } from "$lib/code/codingEngineClient";
  import type { CodeLanguageNavigationKind } from "$lib/code/codeLanguageNavigation";
  import { codeWorkbenchState } from "$lib/code/codeWorkbenchState.svelte";
  import { codeWorkspace, type CodeDocumentTab } from "$lib/stores/codeWorkspace.svelte";

  type EditorSelection = {
    startLine: number;
    endLine: number;
    text: string;
  };

  interface Props {
    workId: string;
    activeTab: CodeDocumentTab | null;
    surfaceError: string | null;
    landError: string | null;
    needsProvision: boolean;
    onProvision?: () => void | Promise<void>;
    externalVersions: Record<string, ForgeSourceFile>;
    editor: CodeMirrorHost | undefined;
    editorSelection: EditorSelection | null;
    editorPrefsEpoch: number;
    documentUri: string | null;
    lspClient: LSPClient | null | undefined;
    bufferInteractive: boolean;
    reviewChangedLines: Array<{ line: number; kind: string }>;
    editorConventions: { indent_style?: "space" | "tab"; indent_size?: string; tab_width?: string };
    wordWrap: boolean;
    showLineNumbers: boolean;
    save: CodeSaveController;
    busy: boolean;
    agentHasControl: boolean;
    onHandoffToAgent?: (runtime: "codex" | "cursor", draft?: string) => Promise<void>;
    problems: CodeProblemsController;
    canReference: boolean;
    canRename: boolean;
    editable: boolean;
    languageActionRunning: boolean;
    onLanguageAction: (
      action: "format" | "organize_imports" | "references" | "rename",
    ) => void;
    onBeginRename: () => void;
    onCursorChanged: (
      tab: CodeDocumentTab,
      cursor: { line: number; totalLines: number; column: number },
    ) => void;
    onProblemsChanged: () => void;
    onContextMenu: (event: MouseEvent) => void;
    onLanguageNavigation: (kind: CodeLanguageNavigationKind) => void;
    symbols: CodeDocumentSymbol[];
    symbolsLoading: boolean;
    references: Array<{ uri?: string; range?: { start?: { line?: number } } }>;
    activeTabLanguage: string;
    activeLspLanguage: string;
    activeLanguageMatrix: CodeLanguageMatrixEntry | null;
    pathFromUri: (uri?: string) => string | null;
    onRestartLanguage: () => void;
    onOpenLocation: (path: string, line: number) => void;
    tasks: CodeTasksController;
    searchOpen: boolean;
    onOpenSearchHit: (path: string, line: number) => void;
    onToggleSearch: (open: boolean) => void;
    onSearchApplied: () => void | Promise<void>;
    quick: CodeQuickOpenController;
    changes: CodeChangesController;
    comparingTabId: string | null;
    onUseProjectVersion: (tab: CodeDocumentTab) => void;
    onKeepDraft: (tab: CodeDocumentTab) => void;
    terminalDockOpen: boolean;
    dockSessionId: string | null;
    workspaceRoot: string | null;
    terminalTitle: string;
    terminalAvailable: boolean;
    dockBusy: boolean;
    onToggleTerminal: (forceOpen?: boolean) => void | Promise<void>;
    onPopOutTerminal: () => void | Promise<void>;
  }

  let {
    workId,
    activeTab,
    surfaceError,
    landError,
    needsProvision,
    onProvision,
    externalVersions,
    editor = $bindable(),
    editorSelection = $bindable(),
    editorPrefsEpoch,
    documentUri,
    lspClient,
    bufferInteractive,
    reviewChangedLines,
    editorConventions,
    wordWrap,
    showLineNumbers,
    save,
    busy,
    agentHasControl,
    onHandoffToAgent,
    problems,
    canReference,
    canRename,
    editable,
    languageActionRunning,
    onLanguageAction,
    onBeginRename,
    onCursorChanged,
    onProblemsChanged,
    onContextMenu,
    onLanguageNavigation,
    symbols,
    symbolsLoading,
    references,
    activeTabLanguage,
    activeLspLanguage,
    activeLanguageMatrix,
    pathFromUri,
    onRestartLanguage,
    onOpenLocation,
    tasks,
    searchOpen,
    onOpenSearchHit,
    onToggleSearch,
    onSearchApplied,
    quick,
    changes,
    comparingTabId = $bindable(),
    onUseProjectVersion,
    onKeepDraft,
    terminalDockOpen = $bindable(),
    dockSessionId,
    workspaceRoot,
    terminalTitle,
    terminalAvailable,
    dockBusy,
    onToggleTerminal,
    onPopOutTerminal,
  }: Props = $props();
</script>

{#if activeTab}
  {#if surfaceError || activeTab.error || codeWorkspace.workspaceErrorByWorkId[workId]}
    <p class="shrink-0 border-b border-amber-500/30 bg-amber-950/25 px-2.5 py-1.5 text-chrome-sm text-amber-100">
      {humanizeForgeMessage(surfaceError || activeTab.error || codeWorkspace.workspaceErrorByWorkId[workId] || "")}
    </p>
  {/if}
  {#if activeTab.preview}
    <div class="flex shrink-0 items-center gap-2 border-b border-sky-500/25 bg-sky-950/20 px-2.5 py-1.5 text-chrome-sm text-sky-100/90" role="status">
      {#if activeTab.encoding === "binary"}
        <span>Binary preview · {activeTab.byte_size.toLocaleString()} bytes · read-only</span>
      {:else if activeTab.truncated}
        <span>Large file preview (first 2 MiB of {activeTab.byte_size.toLocaleString()} bytes) · read-only</span>
      {:else}
        <span>Lossy text preview ({activeTab.encoding ?? "unknown"} encoding) · read-only</span>
      {/if}
    </div>
  {/if}
  {#if externalVersions[activeTab.tabId]}
    <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-amber-500/30 bg-amber-950/20 px-2.5 py-1.5 text-chrome-sm text-amber-100">
      <span class="min-w-40 flex-1">This file changed elsewhere. Your draft is safe.</span>
      <button type="button" class="rounded px-1.5 py-0.5 hover:bg-white/10" onclick={() => (comparingTabId = activeTab.tabId)}>Compare</button>
      <button type="button" class="rounded px-1.5 py-0.5 hover:bg-white/10" onclick={() => onUseProjectVersion(activeTab)}>Use project version</button>
      <button type="button" class="rounded bg-amber-500/20 px-1.5 py-0.5" onclick={() => onKeepDraft(activeTab)}>Keep my draft</button>
    </div>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div class="relative min-h-0 min-w-0 flex-1">
      {#if editorSelection?.text && onHandoffToAgent && !agentHasControl}
        <div class="absolute right-3 top-2 z-20 flex max-w-[calc(100%-1.5rem)] items-center gap-1 overflow-x-auto rounded-md border border-primary-500/30 bg-surface-950/95 px-1.5 py-1 shadow-xl" aria-label="Selected code actions">
          <span class="mr-1 flex shrink-0 items-center gap-1 text-chrome-xs text-primary-200/80"><Sparkles size={10} />Selection</span>
          <button type="button" class="code-intent-action" disabled={busy} onclick={() => void save.handoffToAgent("Help me understand the selected code and answer my questions about it.")}>Ask</button>
          <button type="button" class="code-intent-action" disabled={busy} onclick={() => void save.handoffToAgent("Change the selected code. Ask only if the intended change is ambiguous.")}>Change</button>
          {#if problems.documentProblems.length > 0}<button type="button" class="code-intent-action" disabled={busy} onclick={() => void save.handoffToAgent("Fix the relevant issue in the selected code and verify the result.")}>Fix</button>{/if}
          <button type="button" class="code-intent-action" disabled={busy} onclick={() => void save.handoffToAgent("Explain the selected code clearly, including its role and important behavior.")}>Explain</button>
          <button type="button" class="code-intent-action" disabled={busy} onclick={() => void save.handoffToAgent("Add the most valuable focused test for the selected code and run the relevant check.")}>Add test</button>
          {#if canReference}<button type="button" class="code-intent-action" disabled={languageActionRunning} onclick={() => void onLanguageAction("references")}>Find uses</button>{/if}
          {#if canRename && editable}<button type="button" class="code-intent-action" disabled={languageActionRunning} onclick={onBeginRename}>Rename</button>{/if}
        </div>
      {/if}
      {#if activeTab.loading}
        <div class="absolute inset-0 z-10 flex items-center justify-center bg-surface-950/70 text-xs text-content-tertiary">
          <LoaderCircle size={14} class="mr-2 animate-spin" />Opening source…
        </div>
      {/if}
      {#if !activeTab.loading && activeTab.digest}
        {@const editorTab = activeTab}
        {#key `${editorTab.tabId}:${editorPrefsEpoch}`}
          <CodeMirrorHost
            bind:this={editor}
            value={editorTab.draft}
            languageId={editorTab.encoding === "binary" ? "plaintext" : editorTab.language}
            {documentUri}
            lspLanguageId={editorTab.preview ? null : codeEditorLspLanguageId(editorTab.language)}
            client={editorTab.preview ? null : lspClient}
            readOnly={!bufferInteractive || editorTab.preview}
            contentSyncKey={editorTab.syncKey}
            changedLines={reviewChangedLines}
            conventionIndentStyle={editorConventions.indent_style ?? null}
            conventionTabSize={Number.parseInt(editorConventions.indent_size ?? editorConventions.tab_width ?? "", 10) || null}
            {wordWrap}
            {showLineNumbers}
            onchange={(value) => void save.onDraftChanged(editorTab.tabId, value)}
            onCursorChanged={(cursor) => onCursorChanged(editorTab, cursor)}
            onSelectionChanged={(selection) => (editorSelection = selection.text ? selection : null)}
            onProblemsChanged={onProblemsChanged}
            onContextMenu={onContextMenu}
            onLanguageNavigationRequested={(kind) => void onLanguageNavigation(kind)}
          />
        {/key}
      {:else if !activeTab.loading}
        <div class="flex h-full min-h-48 items-center justify-center p-6 text-xs text-content-quiet">
          This file could not be opened.
        </div>
      {/if}
    </div>
  </div>

  <CodeContextSidePanel
    {problems}
    {symbols}
    {symbolsLoading}
    {references}
    {workId}
    {documentUri}
    languageId={activeTabLanguage}
    lspLanguageId={activeLspLanguage}
    languageMatrix={activeLanguageMatrix}
    {pathFromUri}
    onRevealLine={(line) => editor?.revealLine(line)}
    onOpenReference={(path, line) => {
      problems.setPanel(null);
      void onOpenLocation(path, line);
    }}
    onRestartLanguage={onRestartLanguage}
  />
  <CodeTasksOutput {tasks} onOpenLocation={(path, line) => void onOpenLocation(path, line)} />
  {#if searchOpen}
    <CodeWorkspaceSearch
      {workId}
      onOpenHit={onOpenSearchHit}
      onClose={() => onToggleSearch(false)}
      onApplied={async () => {
        await onSearchApplied();
      }}
    />
  {/if}
  {#if changes.open}
    <CodeChangesPanel
      changes={changes.changes}
      loading={changes.loading}
      error={changes.error}
      selectedPath={changes.selectedPath}
      fileDiff={changes.fileDiff}
      fileLoading={changes.fileLoading}
      fileError={changes.fileError}
      restoreBusy={changes.restoreBusy}
      syncBusy={changes.syncBusy}
      syncMessage={changes.syncMessage}
      history={changes.history}
      historyOpen={changes.historyOpen}
      blameHunks={changes.blameHunks}
      blameOpen={changes.blameOpen}
      onSelectPath={(path) => void changes.selectPath(path)}
      onOpenPath={(path, line) => void onOpenLocation(path, line ?? 1)}
      onRestorePath={(diff) => void changes.restoreFile(diff)}
      onRevertHunk={(diff, hunkIndex) => void changes.revertHunk(diff, hunkIndex)}
      onResolveConflict={(diff, resolution) => void changes.resolveConflict(diff, resolution)}
      onFetch={() => void changes.runSync("fetch")}
      onPull={() => void changes.runSync("pull")}
      onPush={() => void changes.runSync("push")}
      onSync={() => void changes.runSync("sync")}
      onCheckpoint={() => void changes.sealForReview()}
      onToggleHistory={() => void changes.toggleHistory()}
      onToggleBlame={() => void changes.toggleBlame()}
      onClose={() => void changes.toggle(false)}
      onRefresh={() => void changes.refresh()}
    />
  {/if}

{:else}
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex min-h-72 flex-1 items-center justify-center p-8">
      {#if needsProvision}
        <EmptyState
          title="Set up this project"
          description={landError
            ? humanizeForgeMessage(landError)
            : "Create the working copy so the tree and editor can open."}
        >
          {#if onProvision}
            <button
              type="button"
              class="rounded bg-primary-500/80 px-3 py-1.5 text-chrome-md font-medium text-surface-50"
              onclick={() => void onProvision()}
            >Set up project</button>
          {/if}
        </EmptyState>
      {:else if landError}
        <EmptyState
          title="Could not open the working set"
          description={humanizeForgeMessage(landError)}
        >
          {#if terminalAvailable}
            <button
              type="button"
              class="rounded border border-surface-500/40 px-3 py-1.5 text-chrome-md text-surface-200 hover:bg-surface-800"
              disabled={dockBusy}
              onclick={() => void onToggleTerminal(true)}
            >Terminal</button>
          {/if}
        </EmptyState>
      {:else}
        <EmptyState
          title="Open a file"
          description="Jump in with Quick Open, or pick a path from the project tree."
        >
          <div class="flex flex-wrap items-center justify-center gap-2">
            <button
              type="button"
              class="rounded bg-primary-500/80 px-3 py-1.5 text-chrome-md font-medium text-surface-50"
              onclick={() => void quick.show()}
            >Open file</button>
            {#if terminalAvailable}
              <button
                type="button"
                class="rounded border border-surface-500/40 px-3 py-1.5 text-chrome-md text-surface-200 hover:bg-surface-800"
                disabled={dockBusy}
                onclick={() => void onToggleTerminal(true)}
              >Terminal</button>
            {/if}
          </div>
        </EmptyState>
      {/if}
    </div>
  </div>
{/if}
<CodeTerminalDock
  open={terminalDockOpen}
  sessionId={dockSessionId}
  {workId}
  worktreeRoot={workspaceRoot}
  title={terminalTitle}
  onClose={() => {
    terminalDockOpen = false;
    if (workId) {
      codeWorkbenchState.setTerminalOpen(workId, false);
      codeWorkspace.scheduleLayoutPersist(workId);
    }
  }}
  onPopOut={() => void onPopOutTerminal()}
/>

<style>
  .code-intent-action {
    border-radius: 0.25rem;
    padding: 0.2rem 0.45rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.625rem;
    white-space: nowrap;
  }

  .code-intent-action:hover:not(:disabled) {
    background: rgb(var(--color-surface-800));
    color: rgb(var(--color-surface-50));
  }

  .code-intent-action:disabled {
    opacity: 0.4;
  }
</style>
