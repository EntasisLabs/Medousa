<script lang="ts">
  /**
   * Shared Code context dock: Problems, language server, uses, and structure.
   */
  import CodePanelFrame from "./CodePanelFrame.svelte";
  import CodeOperationNotice from "./CodeOperationNotice.svelte";
  import { captureCodeScope } from "$lib/code/codeWorkspaceContext.svelte";
  import { onDestroy, untrack } from "svelte";
  import {
    FileCode2,
    ListTree,
    LoaderCircle,
    RotateCcw,
    X,
  } from "@lucide/svelte";
  import type { CodeProblemsController } from "$lib/code/codeProblemsController.svelte";
  import {
    getCodeLanguageSessions,
    type CodeDocumentSymbol,
    type CodeLanguageMatrixEntry,
    type CodeLanguageSessionSnapshot,
    type CodeWorkspaceLspStatus,
  } from "$lib/code/codingEngineClient";
  import { languageSupportsLsp } from "$lib/code/codeEditorLanguageRegistry";

  type ReferenceHit = { uri?: string; range?: { start?: { line?: number } } };

  interface Props {
    problems: CodeProblemsController;
    symbols: CodeDocumentSymbol[];
    symbolsLoading?: boolean;
    references: ReferenceHit[];
    workId: string;
    workspaceScope: string;
    languageStatus: CodeWorkspaceLspStatus;
    languageError: string | null;
    onLanguagePackages: () => void;
    documentUri: string | null;
    languageId: string;
    lspLanguageId: string;
    languageMatrix: CodeLanguageMatrixEntry | null;
    pathFromUri: (uri?: string) => string | null;
    onRevealLine: (line: number) => void;
    onOpenReference: (path: string, line: number) => void;
    onRestartLanguage: () => void;
  }

  let {
    problems,
    symbols,
    symbolsLoading = false,
    references,
    workId,
    workspaceScope,
    languageStatus,
    languageError,
    onLanguagePackages,
    documentUri,
    languageId,
    lspLanguageId,
    languageMatrix,
    pathFromUri,
    onRevealLine,
    onOpenReference,
    onRestartLanguage,
  }: Props = $props();

  let requestEpoch = 0;
  let disposed = false;
  onDestroy(() => { disposed = true; requestEpoch += 1; });
  let languageSessions = $state<CodeLanguageSessionSnapshot[]>([]);
  let languageSessionsLoading = $state(false);
  let languageSessionsError = $state<string | null>(null);

  const latestSession = $derived(
    languageSessions.find((session) => session.kind === "editor") ?? languageSessions[0] ?? null,
  );
  const languageLogs = $derived.by(() =>
    languageSessions
      .flatMap((session) =>
        session.logs.map((entry) => ({ ...entry, sessionId: session.id })),
      )
      .sort((a, b) => a.timestamp_ms - b.timestamp_ms || a.sequence - b.sequence)
      .slice(-500),
  );

  function symbolLine(symbol: CodeDocumentSymbol): number {
    return (
      symbol.selectionRange?.start?.line ?? symbol.range?.start?.line ?? 0
    ) + 1;
  }

  function contextPanelOpen(): boolean {
    return problems.panel !== null && problems.panel !== "problems";
  }

  function formatLanguageLogTime(timestamp: number): string {
    return new Date(timestamp).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
  }

  async function refreshLanguageSessions(options?: { quiet?: boolean }) {
    if (!workId || !documentUri || !languageSupportsLsp(languageId)) {
      languageSessions = [];
      languageSessionsError = null;
      return;
    }
    const epoch = ++requestEpoch;
    const current = captureCodeScope(() => JSON.stringify([workspaceScope, documentUri, lspLanguageId]));
    if (!options?.quiet) languageSessionsLoading = true;
    try {
      const snapshot = await getCodeLanguageSessions({
        workId,
        uri: documentUri,
        language: lspLanguageId,
      });
      if (disposed || !current() || epoch !== requestEpoch) return;
      languageSessions = snapshot.sessions;
      languageSessionsError = null;
    } catch (err) {
      if (disposed || !current() || epoch !== requestEpoch) return;
      languageSessionsError = err instanceof Error ? err.message : String(err);
    } finally {
      if (!disposed && current() && epoch === requestEpoch) languageSessionsLoading = false;
    }
  }

  $effect(() => {
    const showingLanguage = problems.panel === "language";
    void workspaceScope; void languageId; void documentUri;
    if (!showingLanguage || !workId || !documentUri) return;
    untrack(() => { languageSessions = []; languageSessionsError = null; void refreshLanguageSessions(); });
    return () => { requestEpoch += 1; };

  });
</script>


{#if contextPanelOpen()}
  <CodePanelFrame {workId} name="Language and structure">
  <div class="min-h-0 flex-1 overflow-y-auto">
    <div class="sticky top-0 z-10 flex items-center justify-between border-b border-surface-500/25 bg-surface-950 px-2 py-1">
      <div class="flex min-w-0 items-center gap-2">
        <span class="text-chrome-xs font-medium uppercase tracking-wider text-content-tertiary">
          {problems.panel === "references" ? "Uses" : problems.panel === "language" ? "Language assistance" : "Structure"}
        </span>
      </div>
      <div class="flex items-center gap-0.5">
        {#if problems.panel === "language"}
          <button
            type="button"
            class="rounded p-0.5 text-content-quiet hover:bg-surface-800 hover:text-surface-200 disabled:opacity-50"
            aria-label="Refresh language server logs"
            title="Refresh language server logs"
            disabled={languageSessionsLoading}
            onclick={() => void refreshLanguageSessions()}
          ><RotateCcw size={11} class={languageSessionsLoading ? "animate-spin" : ""} /></button>
        {/if}
        <button type="button" class="rounded p-0.5 text-content-quiet hover:text-surface-200" aria-label="Close context panel" onclick={() => problems.setPanel(null)}><X size={11} /></button>
      </div>
    </div>
    {#if problems.panel === "language"}
      <div class="flex flex-wrap items-center gap-2 border-b border-surface-500/20 bg-surface-900/55 px-3 py-2 text-chrome-sm text-content-secondary">
        <span class="font-medium">{languageId}</span>
        <span class="rounded bg-surface-800 px-2 py-1 text-chrome-xs">{languageStatus.phase === "ready" ? "Ready for this file" : languageStatus.phase === "stopped" ? "Editing only" : languageStatus.phase}</span>
        <button type="button" class="rounded bg-surface-800 px-2 py-1 text-chrome-xs" onclick={onRestartLanguage}>Restart this service</button>
        <button type="button" class="rounded bg-surface-800 px-2 py-1 text-chrome-xs" onclick={onLanguagePackages}>Settings → Packages</button>
      </div>
      <p class="px-3 py-2 text-chrome-sm text-content-secondary">{languageStatus.phase === "ready" ? "Language assistance is connected for this file. Other files may need their own language sessions." : "Your file remains editable with syntax highlighting. Language assistance is not ready for this file."}</p>
      {#if languageError}<CodeOperationNotice message={languageError} />{/if}
      <details class="border-t border-surface-500/20"><summary class="cursor-pointer px-3 py-2 text-chrome-sm text-content-quiet">Service details and logs</summary>
        <p class="px-3 py-2 font-mono text-chrome-xs text-content-quiet">{languageStatus.detail}{languageMatrix?.command ? ` · ${languageMatrix.command}` : ""}{languageMatrix?.packageId ? ` · ${languageMatrix.packageId}` : ""}{latestSession ? ` · ${latestSession.language_root} · ${latestSession.phase}` : " · No workshop snapshot yet"}</p>
      {#if latestSession?.progress.some((progress) => !progress.done)}
        {#each latestSession.progress.filter((progress) => !progress.done) as progress (progress.token)}
          <div class="flex items-center gap-2 border-b border-sky-500/15 bg-sky-950/10 px-3 py-1.5 text-chrome-xs text-sky-100/80">
            <LoaderCircle size={10} class="animate-spin" />
            <span class="min-w-0 flex-1 truncate">{progress.title || "Language service"}{progress.message ? ` · ${progress.message}` : ""}</span>
            {#if progress.percentage != null}<span>{Math.round(progress.percentage)}%</span>{/if}
          </div>
        {/each}
      {/if}
      {#if languageSessionsError}
        <div class="flex items-start justify-between gap-3 border-b border-rose-400/20 bg-rose-500/5 px-3 py-2 text-chrome-sm text-rose-200">
          <span>Could not read workshop language logs: {languageSessionsError}</span>
          <button type="button" class="shrink-0 underline underline-offset-2" onclick={() => void refreshLanguageSessions()}>Retry</button>
        </div>
      {/if}
      {#if languageSessionsLoading && languageSessions.length === 0}
        <p class="flex items-center px-3 py-3 text-chrome-sm text-content-quiet"><LoaderCircle size={11} class="mr-1.5 animate-spin" />Reading workshop logs…</p>
      {:else if languageLogs.length === 0}
        <p class="px-3 py-3 text-chrome-sm text-content-quiet">No language server output has been recorded.</p>
      {:else}
        <div class="font-mono text-chrome-xs" aria-label="Language server output">
          {#each languageLogs as entry (`${entry.sessionId}:${entry.sequence}`)}
            <div class="grid grid-cols-[4.8rem_3.5rem_minmax(0,1fr)] gap-2 border-b border-surface-500/10 px-3 py-1 {entry.level === 'error' ? 'text-rose-200' : entry.level === 'warning' ? 'text-amber-200' : 'text-content-tertiary'}">
              <span class="text-content-faint">{formatLanguageLogTime(entry.timestamp_ms)}</span>
              <span class="truncate text-content-quiet">{entry.source}</span>
              <span class="whitespace-pre-wrap break-words">{entry.message}</span>
            </div>
          {/each}
        </div>
      {/if}
      </details>
    {:else if problems.panel === "references"}
      {#if references.length === 0}
        <p class="px-3 py-3 text-chrome-sm text-content-quiet">No other uses found.</p>
      {:else}
        {#each references as reference, index (`${reference.uri}:${reference.range?.start?.line}:${index}`)}
          {@const referencePath = pathFromUri(reference.uri)}
          {@const referenceLine = (reference.range?.start?.line ?? 0) + 1}
          <button
            type="button"
            class="flex w-full items-center gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
            onclick={() => {
              if (!referencePath) return;
              onOpenReference(referencePath, referenceLine);
            }}
          >
            <FileCode2 size={11} class="shrink-0 text-content-link/70" />
            <span class="min-w-0 flex-1 truncate text-chrome-sm text-content-secondary">{referencePath ?? reference.uri}</span>
            <span class="font-mono text-chrome-xs text-content-quiet">{referenceLine}</span>
          </button>
        {/each}
      {/if}
    {:else if symbolsLoading}
      <p class="px-3 py-3 text-chrome-sm text-content-quiet">Reading file structure…</p>
    {:else if symbols.length === 0}
      <p class="px-3 py-3 text-chrome-sm text-content-quiet">No structure is available for this file.</p>
    {:else}
      {#each symbols as symbol (`${symbol.name}:${symbolLine(symbol)}`)}
        <button
          type="button"
          class="flex w-full items-center gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
          onclick={() => onRevealLine(symbolLine(symbol))}
        >
          <ListTree size={11} class="shrink-0 text-content-link/70" />
          <span class="min-w-0 flex-1 truncate text-chrome-sm text-content-secondary">{symbol.name}</span>
          <span class="font-mono text-chrome-xs text-content-quiet">{symbolLine(symbol)}</span>
        </button>
      {/each}
    {/if}
  </div>
  </CodePanelFrame>
{/if}
