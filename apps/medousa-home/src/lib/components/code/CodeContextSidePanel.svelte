<script lang="ts">
  /**
   * Shared Code context dock: Problems, language server, uses, and structure.
   */
  import { untrack } from "svelte";
  import {
    CircleAlert,
    FileCode2,
    ListTree,
    LoaderCircle,
    RotateCcw,
    Search,
    X,
  } from "@lucide/svelte";
  import {
    PROBLEM_SEVERITY_OPTIONS,
    type CodeProblemsController,
  } from "$lib/code/codeProblemsController.svelte";
  import {
    getCodeLanguageSessions,
    isPermanentLanguageServiceError,
    type CodeDocumentSymbol,
    type CodeLanguageMatrixEntry,
    type CodeLanguageSessionSnapshot,
  } from "$lib/code/codingEngineClient";
  import { languageSupportsLsp } from "$lib/code/codeEditorLanguageRegistry";

  type ReferenceHit = { uri?: string; range?: { start?: { line?: number } } };

  interface Props {
    problems: CodeProblemsController;
    symbols: CodeDocumentSymbol[];
    symbolsLoading?: boolean;
    references: ReferenceHit[];
    workId: string;
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
    documentUri,
    languageId,
    lspLanguageId,
    languageMatrix,
    pathFromUri,
    onRevealLine,
    onOpenReference,
    onRestartLanguage,
  }: Props = $props();

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
    if (!options?.quiet) languageSessionsLoading = true;
    try {
      const snapshot = await getCodeLanguageSessions({
        workId,
        uri: documentUri,
        language: lspLanguageId,
      });
      languageSessions = snapshot.sessions;
      languageSessionsError = null;
    } catch (err) {
      languageSessionsError = err instanceof Error ? err.message : String(err);
    } finally {
      languageSessionsLoading = false;
    }
  }

  $effect(() => {
    const showingLanguage = problems.panel === "language";
    const scope = `${workId}:${languageId}:${documentUri ?? ""}`;
    void scope;
    if (!showingLanguage || !workId || !documentUri) return;
    untrack(() => void refreshLanguageSessions());
    if (
      languageSessionsError &&
      isPermanentLanguageServiceError(languageSessionsError)
    ) {
      return;
    }
    const timer = setInterval(
      () => untrack(() => void refreshLanguageSessions({ quiet: true })),
      1_500,
    );
    return () => clearInterval(timer);
  });
</script>


{#if problems.panel}
  <div class="{problems.panel === 'problems' || problems.panel === 'language' ? 'max-h-72' : 'max-h-44'} shrink-0 overflow-y-auto border-t border-surface-500/30 bg-surface-950/80">
    <div class="sticky top-0 z-10 flex items-center justify-between border-b border-surface-500/25 bg-surface-950 px-2 py-1">
      <div class="flex min-w-0 items-center gap-2">
        <span class="text-chrome-xs font-medium uppercase tracking-wider text-content-tertiary">
          {problems.panel === "problems" ? "Problems" : problems.panel === "references" ? "Uses" : problems.panel === "language" ? "Language server" : "Structure"}
        </span>
        {#if problems.panel === "problems"}
          <span class="text-chrome-xs text-rose-300" title="Errors">{problems.counts.errors}</span>
          <span class="text-chrome-xs text-amber-300" title="Warnings">{problems.counts.warnings}</span>
          <span class="text-chrome-xs text-content-quiet" title="Information and hints">{problems.counts.information + problems.counts.hints}</span>
        {/if}
      </div>
      <div class="flex items-center gap-0.5">
        {#if problems.panel === "problems"}
          <button
            type="button"
            class="rounded p-0.5 text-content-quiet hover:bg-surface-800 hover:text-surface-200 disabled:opacity-50"
            aria-label="Refresh project problems"
            title="Refresh project problems"
            disabled={problems.loading}
            onclick={() => void problems.refresh()}
          ><RotateCcw size={11} class={problems.loading ? "animate-spin" : ""} /></button>
        {:else if problems.panel === "language"}
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
    {#if problems.panel === "problems"}
      <div class="flex items-center gap-1.5 border-b border-surface-500/20 px-2 py-1.5">
        <label class="flex min-w-28 flex-1 items-center gap-1.5 rounded border border-surface-500/35 bg-surface-900/70 px-1.5 py-1 focus-within:border-primary-400/60">
          <Search size={11} class="shrink-0 text-content-quiet" />
          <input
            class="min-w-0 flex-1 bg-transparent text-chrome-sm text-content-secondary outline-none placeholder:text-content-faint"
            aria-label="Filter project problems"
            placeholder="Filter problems"
            bind:value={problems.query}
          />
        </label>
        <div class="flex items-center gap-0.5" aria-label="Problem severity filter">
          {#each PROBLEM_SEVERITY_OPTIONS as option (option.value)}
            <button
              type="button"
              class="rounded px-1.5 py-1 text-chrome-xs {problems.severity === option.value ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800 hover:text-content-secondary'}"
              aria-pressed={problems.severity === option.value}
              onclick={() => (problems.severity = option.value)}
            >{option.label}</button>
          {/each}
        </div>
      </div>
      {#if problems.error}
        <div class="flex items-start justify-between gap-3 border-b border-rose-400/20 bg-rose-500/5 px-3 py-2 text-chrome-sm text-rose-200">
          <span>Could not refresh project problems: {problems.error}</span>
          <button type="button" class="shrink-0 underline underline-offset-2" onclick={() => void problems.refresh()}>Retry</button>
        </div>
      {/if}
      {#if problems.unavailableLanguages.length > 0}
        <p class="border-b border-amber-400/20 bg-amber-500/5 px-3 py-1.5 text-chrome-sm text-amber-200">
          Results are incomplete for {problems.unavailableLanguages.join(", ")}.
        </p>
      {/if}
      {#if problems.loading && !problems.loaded}
        <p class="flex items-center px-3 py-3 text-chrome-sm text-content-quiet"><LoaderCircle size={11} class="mr-1.5 animate-spin" />Loading project problems…</p>
      {:else if problems.counts.total === 0}
        <p class="px-3 py-3 text-chrome-sm text-content-quiet">No problems found in this project.</p>
      {:else if problems.groups.length === 0}
        <p class="px-3 py-3 text-chrome-sm text-content-quiet">No problems match the current filters.</p>
      {:else}
        {#each problems.groups as group (group.path)}
          <div class="flex items-center gap-2 border-b border-surface-500/20 bg-surface-900/70 px-3 py-1 text-chrome-xs text-content-tertiary">
            <FileCode2 size={10} class="shrink-0 text-content-link/70" />
            <span class="min-w-0 flex-1 truncate font-mono">{group.path}</span>
            {#if group.counts.errors > 0}<span class="text-rose-300">{group.counts.errors}E</span>{/if}
            {#if group.counts.warnings > 0}<span class="text-amber-300">{group.counts.warnings}W</span>{/if}
            {#if group.counts.information + group.counts.hints > 0}<span>{group.counts.information + group.counts.hints}I</span>{/if}
          </div>
          {#each group.problems as problem (problem.id)}
            <button
              type="button"
              class="flex w-full items-start gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
              title={`${problem.path}:${problem.line}:${problem.character}`}
              onclick={() => void problems.openProblem(problem)}
            >
              <CircleAlert
                size={11}
                class={problem.severity === "error"
                  ? "mt-0.5 shrink-0 text-rose-300"
                  : problem.severity === "warning"
                    ? "mt-0.5 shrink-0 text-amber-300"
                    : "mt-0.5 shrink-0 text-sky-300"}
              />
              <span class="min-w-0 flex-1 text-chrome-sm text-content-secondary">
                <span class="break-words">{problem.message}</span>
                {#if problem.source || problem.code}
                  <span class="ml-1 text-chrome-xs text-content-faint">{[problem.source, problem.code].filter(Boolean).join(" · ")}</span>
                {/if}
              </span>
              <span class="shrink-0 font-mono text-chrome-xs text-content-quiet">{problem.line}:{problem.character}</span>
            </button>
          {/each}
        {/each}
      {/if}
    {:else if problems.panel === "language"}
      <div class="flex flex-wrap items-center gap-2 border-b border-surface-500/20 bg-surface-900/55 px-3 py-2 text-chrome-sm text-content-secondary">
        <span class="font-medium">{languageId}</span>
        {#if languageMatrix}
          <span class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs {languageMatrix.usable ? 'text-emerald-200' : 'text-rose-200'}">{languageMatrix.usable ? "usable" : "missing"}</span>
          {#if languageMatrix.command}
            <span class="font-mono text-chrome-xs text-content-quiet">{languageMatrix.command}</span>
          {/if}
          {#if languageMatrix.packageId}
            <span class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs text-content-quiet">pkg:{languageMatrix.packageId}</span>
          {/if}
        {/if}
        {#if latestSession}
          <span class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs {latestSession.phase === 'failed' ? 'text-rose-200' : latestSession.phase === 'ready' ? 'text-emerald-200' : 'text-amber-200'}">{latestSession.phase}</span>
          <span class="min-w-0 flex-1 truncate font-mono text-chrome-xs text-content-quiet" title={latestSession.language_root}>{latestSession.relative_root || "."}</span>
        {:else}
          <span class="min-w-0 flex-1 text-content-quiet">No workshop session snapshot yet</span>
        {/if}
        <button type="button" class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs hover:bg-surface-700" onclick={onRestartLanguage}>Restart</button>
      </div>
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
{/if}
