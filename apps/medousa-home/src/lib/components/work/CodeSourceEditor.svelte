<script lang="ts">
  import { tick, untrack } from "svelte";
  import {
    CircleAlert,
    Columns2,
    FileCode2,
    ListTree,
    LoaderCircle,
    RotateCcw,
    Save,
    SquareTerminal,
    GitPullRequestArrow,
    Orbit,
    X,
  } from "@lucide/svelte";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import CodeSplitEditorPane from "$lib/components/work/CodeSplitEditorPane.svelte";
  import type { LSPClient } from "@codemirror/lsp-client";
  import {
    getCodeWorkspaceLspClient,
    getCodeDocumentSymbols,
    pathToFileUri,
    type CodeDocumentSymbol,
  } from "$lib/code/codingEngineClient";
  import { languageSupportsLsp } from "$lib/code/codeEditorLanguageRegistry";
  import {
    beginHumanAttempt,
    getUndertakingSource,
    heartbeatLease,
    humanizeForgeMessage,
    saveUndertakingSource,
  } from "$lib/forge";
  import { codeWorkspace, type CodeDocumentTab } from "$lib/stores/codeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";

  interface Props {
    fill?: boolean;
    worldOpen?: boolean;
    reviewAvailable?: boolean;
    terminalAvailable?: boolean;
    onToggleWorld?: () => void;
    onOpenReview?: () => void;
    onOpenTerminal?: () => void;
  }

  let {
    fill = false,
    worldOpen = false,
    reviewAvailable = false,
    terminalAvailable = false,
    onToggleWorld,
    onOpenReview,
    onOpenTerminal,
  }: Props = $props();

  let saving = $state(false);
  let surfaceError = $state<string | null>(null);
  let editor = $state<CodeMirrorHost | undefined>();
  let lspClient = $state<LSPClient | null>(null);
  let lspError = $state<string | null>(null);
  let lspConnecting = $state(false);
  let contextPanel = $state<"problems" | "outline" | null>(null);
  let problems = $state<ReturnType<CodeMirrorHost["getProblems"]>>([]);
  let symbols = $state<CodeDocumentSymbol[]>([]);
  let symbolsLoading = $state(false);
  let focusedSide = $state(false);

  const context = $derived(undertakings.active);
  const detail = $derived(undertakings.detail);
  const workId = $derived(detail?.id ?? context?.workId ?? "");
  const tabs = $derived(codeWorkspace.tabs.filter((tab) => tab.work_id === workId));
  const activeTab = $derived.by(() => {
    const activeId = codeWorkspace.activeByWorkId[workId];
    return activeId
      ? (codeWorkspace.tabs.find((tab) => tab.tabId === activeId) ?? null)
      : null;
  });
  const dirty = $derived(Boolean(activeTab && codeWorkspace.isDirty(activeTab)));
  const secondaryTab = $derived(codeWorkspace.secondaryFor(workId));
  const editable = $derived(
    Boolean(
      context?.workId === workId &&
      context.leaseId &&
      context.leaseGeneration != null,
    ),
  );
  const documentUri = $derived.by(() => {
    if (!activeTab || !context?.worktree) return null;
    return pathToFileUri(
      `${context.worktree.replace(/[\\/]$/, "")}/${activeTab.path}`,
    );
  });

  async function openSelectedLocation() {
    const selectedPath = context?.workId === workId ? context.selectedPath : null;
    if (!workId || !selectedPath) return;
    await codeWorkspace.hydrate(workId);
    const tab = await codeWorkspace.open(workId, selectedPath, context?.selectedLine);
    await tick();
    if (tab?.line) editor?.revealLine(tab.line);
  }

  function activate(tab: CodeDocumentTab) {
    codeWorkspace.activate(tab.tabId);
    undertakings.setSelection({ path: tab.path, line: tab.line, entityId: null });
    void tick().then(() => {
      if (tab.line) editor?.revealLine(tab.line);
    });
  }

  function close(tab: CodeDocumentTab) {
    if (
      codeWorkspace.isDirty(tab) &&
      !window.confirm(`Discard unsaved changes to ${tab.path}?`)
    ) return;
    codeWorkspace.close(tab.tabId);
    const next = codeWorkspace.activeFor(tab.work_id);
    undertakings.setSelection({
      path: next?.path ?? null,
      line: next?.line ?? null,
      entityId: null,
    });
  }

  async function reload() {
    const tab = activeTab;
    if (!tab || tab.loading) return;
    if (
      codeWorkspace.isDirty(tab) &&
      !window.confirm(`Discard unsaved changes to ${tab.path}?`)
    ) return;
    await codeWorkspace.reload(tab.tabId, { discardDirty: true });
  }

  async function reconcileExternal(tab: CodeDocumentTab | null) {
    if (!tab || tab.loading || !tab.digest) return;
    try {
      const source = await getUndertakingSource(tab.work_id, tab.path);
      const current = codeWorkspace.tabs.find((entry) => entry.tabId === tab.tabId);
      if (!current || current.digest === source.digest) return;
      if (codeWorkspace.isDirty(current)) {
        codeWorkspace.setError(
          current.tabId,
          "This file changed outside Medousa. Your draft is preserved; review it before reloading or saving.",
        );
      } else {
        codeWorkspace.acceptSaved(current.tabId, source);
      }
    } catch {
      // Tree refresh and explicit reload surface durable errors. Polling stays quiet.
    }
  }

  async function startEditing() {
    if (!detail || !detail.allowed_actions.begin_attempt.allowed) return;
    saving = true;
    surfaceError = null;
    try {
      const begun = await beginHumanAttempt(detail.id);
      undertakings.setActiveFromItem(begun.item, {
        leaseId: begun.lease.lease_id,
        leaseGeneration: begun.lease.generation,
        executorKind: "human",
      });
      await undertakings.refreshDetail();
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function saveTab(tab: CodeDocumentTab | null) {
    const active = context;
    if (
      !tab ||
      !active?.leaseId ||
      active.leaseGeneration == null ||
      !codeWorkspace.isDirty(tab) ||
      saving
    ) return;
    saving = true;
    surfaceError = null;
    codeWorkspace.setError(tab.tabId, null);
    try {
      const next = await saveUndertakingSource(active.workId, {
        path: tab.path,
        content: tab.draft,
        lease_id: active.leaseId,
        generation: active.leaseGeneration,
        expected_digest: tab.digest,
      });
      codeWorkspace.acceptSaved(tab.tabId, next);
    } catch (err) {
      codeWorkspace.setError(
        tab.tabId,
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      saving = false;
    }
  }

  async function save() {
    await saveTab(activeTab);
  }

  async function saveAll() {
    for (const tab of tabs) {
      if (codeWorkspace.isDirty(tab)) await saveTab(tab);
    }
  }

  function cycleTab(direction: 1 | -1) {
    if (tabs.length < 2 || !activeTab) return;
    const index = tabs.findIndex((tab) => tab.tabId === activeTab.tabId);
    const next = tabs[(index + direction + tabs.length) % tabs.length];
    if (next) activate(next);
  }

  function toggleSplit() {
    if (secondaryTab) {
      codeWorkspace.closeSide(workId);
      return;
    }
    const candidate = tabs.find((tab) => tab.tabId !== activeTab?.tabId);
    if (candidate) codeWorkspace.openToSide(candidate.tabId);
  }

  function syncProblems() {
    void tick().then(() => {
      problems = editor?.getProblems() ?? [];
    });
  }

  async function showOutline() {
    contextPanel = contextPanel === "outline" ? null : "outline";
    if (contextPanel !== "outline" || !activeTab || !documentUri || !lspClient) return;
    symbolsLoading = true;
    try {
      symbols = await getCodeDocumentSymbols({
        workId,
        uri: documentUri,
        language: activeTab.language,
      });
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
      symbols = [];
    } finally {
      symbolsLoading = false;
    }
  }

  function symbolLine(symbol: CodeDocumentSymbol): number {
    return (
      symbol.selectionRange?.start?.line ?? symbol.range?.start?.line ?? 0
    ) + 1;
  }

  $effect(() => {
    void context?.workId;
    void context?.selectedPath;
    void context?.selectedLine;
    void workId;
    untrack(() => void openSelectedLocation());
  });

  $effect(() => {
    const primary = activeTab;
    const secondary = secondaryTab;
    if (!primary && !secondary) return;
    const reconcile = () => {
      void reconcileExternal(primary);
      if (secondary?.tabId !== primary?.tabId) void reconcileExternal(secondary);
    };
    const timer = setInterval(reconcile, 4_000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (!workId) return;
    const lease =
      context?.workId === workId && context.leaseId && context.leaseGeneration != null
        ? { lease_id: context.leaseId, generation: context.leaseGeneration }
        : null;
    codeWorkspace.setLease(workId, lease);
    void codeWorkspace.hydrate(workId);
  });

  $effect(() => {
    const tab = activeTab;
    const root = context?.workId === workId ? context.worktree : null;
    if (!tab || !root || !languageSupportsLsp(tab.language)) {
      lspClient = null;
      lspError = null;
      lspConnecting = false;
      return;
    }
    let cancelled = false;
    lspClient = null;
    lspError = null;
    lspConnecting = true;
    void getCodeWorkspaceLspClient({
      workId,
      workspaceRoot: root,
      language: tab.language,
    })
      .then((client) => {
        if (!cancelled) lspClient = client;
      })
      .catch((err) => {
        if (!cancelled) lspError = err instanceof Error ? err.message : String(err);
      })
      .finally(() => {
        if (!cancelled) lspConnecting = false;
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const tab = activeTab;
    if (tab?.line) {
      void tick().then(() => editor?.revealLine(tab.line!));
    }
  });

  $effect(() => {
    const leaseId = context?.workId === workId ? context.leaseId : null;
    const generation = context?.workId === workId ? context.leaseGeneration : null;
    if (!leaseId || generation == null) return;
    const beat = async () => {
      try {
        await heartbeatLease(leaseId, generation);
      } catch (err) {
        surfaceError = err instanceof Error ? err.message : String(err);
        await undertakings.refreshDetail();
      }
    };
    void beat();
    const timer = setInterval(() => void beat(), 30_000);
    return () => clearInterval(timer);
  });
</script>

<section class="flex flex-col overflow-hidden rounded-lg border border-surface-500/35 bg-surface-950/45 {fill ? 'min-h-0 flex-1' : 'min-h-[26rem]'}">
  {#if tabs.length > 0}
    <div class="flex shrink-0 items-end overflow-x-auto border-b border-surface-500/30 bg-surface-950/65 px-1 pt-1" role="tablist" aria-label="Open files">
      {#each tabs as tab (tab.tabId)}
        {@const selected = activeTab?.tabId === tab.tabId}
        <div class="group flex max-w-52 shrink-0 items-center border border-b-0 {selected ? 'border-surface-500/45 bg-surface-900 text-surface-100' : 'border-transparent text-surface-500 hover:bg-surface-900/60 hover:text-surface-300'}">
          <button
            type="button"
            role="tab"
            aria-selected={selected}
            class="flex min-w-0 flex-1 items-center gap-1.5 px-2 py-1.5 text-left text-[10px]"
            title={tab.path}
            onclick={() => activate(tab)}
          >
            <FileCode2 size={11} class="shrink-0 opacity-70" />
            <span class="truncate">{tab.title}</span>
            {#if codeWorkspace.isDirty(tab)}
              <span class="size-1.5 shrink-0 rounded-full bg-primary-300" aria-label="Unsaved changes"></span>
            {/if}
          </button>
          <button
            type="button"
            class="mr-1 rounded p-0.5 opacity-60 hover:bg-surface-700 focus:opacity-100 sm:opacity-0 sm:group-hover:opacity-100"
            aria-label={`Close ${tab.title}`}
            onclick={() => close(tab)}
          ><X size={10} /></button>
          {#if !selected}
            <button
              type="button"
              class="mr-1 rounded p-0.5 opacity-60 hover:bg-surface-700 focus:opacity-100 sm:opacity-0 sm:group-hover:opacity-100"
              aria-label={`Open ${tab.title} to the side`}
              title="Open to side"
              onclick={() => codeWorkspace.openToSide(tab.tabId)}
            ><Columns2 size={10} /></button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if activeTab}
    <header class="flex shrink-0 items-center justify-between gap-1.5 border-b border-surface-500/30 px-2 py-1.5 sm:gap-3 sm:px-2.5">
      <div class="flex min-w-0 items-center gap-2">
        <FileCode2 size={14} class="shrink-0 text-primary-300" />
        <div class="min-w-0">
          <p class="truncate font-mono text-[11px] text-surface-200">{activeTab.path}</p>
          <p class="text-[9px] text-surface-500">
            {activeTab.language}{activeTab.line ? ` · line ${activeTab.line}` : ""}{dirty ? " · unsaved" : ""}
            {#if lspConnecting} · understanding code…{:else if lspError} · editing only{/if}
          </p>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-1">
        <button
          type="button"
          class="rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100 disabled:opacity-35"
          disabled={tabs.length < 2}
          title={secondaryTab ? "Close editor group" : "Open another file to the side"}
          onclick={toggleSplit}
          aria-label={secondaryTab ? "Close editor group" : "Split editor"}
        ><Columns2 size={11} class="sm:mr-1 sm:inline" /><span class="hidden sm:inline">{secondaryTab ? "Unsplit" : "Split"}</span></button>
        <button
          type="button"
          class="rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100 disabled:opacity-40"
          disabled={activeTab.loading || saving}
          onclick={() => void reload()}
          aria-label="Reload file"
        ><RotateCcw size={11} class="sm:mr-1 sm:inline" /><span class="hidden sm:inline">Reload</span></button>
        {#if !editable && detail?.allowed_actions.begin_attempt.allowed}
          <button
            type="button"
            class="rounded bg-primary-500/80 px-2 py-1 text-[10px] font-medium text-surface-50 disabled:opacity-40"
            disabled={saving}
            onclick={() => void startEditing()}
            aria-label="Edit file"
          ><span class="hidden sm:inline">Edit</span><Save size={11} class="sm:hidden" /></button>
        {:else}
          <button
            type="button"
            class="rounded bg-primary-500/80 px-2 py-1 text-[10px] font-medium text-surface-50 disabled:opacity-40"
            disabled={!editable || !dirty || saving}
            onclick={() => void save()}
            aria-label="Save file"
          ><Save size={11} class="sm:mr-1 sm:inline" /><span class="hidden sm:inline">Save</span></button>
        {/if}
      </div>
    </header>

    {#if surfaceError || activeTab.error || codeWorkspace.workspaceErrorByWorkId[workId]}
      <p class="shrink-0 border-b border-amber-500/30 bg-amber-950/25 px-2.5 py-1.5 text-[10px] text-amber-100">
        {humanizeForgeMessage(surfaceError || activeTab.error || codeWorkspace.workspaceErrorByWorkId[workId] || "")}
      </p>
    {/if}

    <div class="min-h-0 flex-1 {secondaryTab ? 'grid grid-cols-1 overflow-y-auto md:grid-cols-2 md:overflow-hidden' : ''}">
      <div class="relative min-h-64 min-w-0 flex-1" onfocusin={() => (focusedSide = false)}>
        {#if activeTab.loading}
          <div class="absolute inset-0 z-10 flex items-center justify-center bg-surface-950/70 text-xs text-surface-400">
            <LoaderCircle size={14} class="mr-2 animate-spin" />Opening source…
          </div>
        {/if}
        {#if !activeTab.loading && activeTab.digest}
          {#key `${activeTab.tabId}:${editable}:${lspClient ? "lsp" : "plain"}`}
            <CodeMirrorHost
              bind:this={editor}
              value={activeTab.draft}
              languageId={activeTab.language}
              {documentUri}
              lspLanguageId={activeTab.language}
              client={lspClient}
              readOnly={!editable}
              contentSyncKey={activeTab.syncKey}
              onchange={(value) => codeWorkspace.updateDraft(activeTab.tabId, value)}
              onCursorChanged={(line) => codeWorkspace.updateLine(activeTab.tabId, line)}
              onProblemsChanged={syncProblems}
            />
          {/key}
        {:else if !activeTab.loading}
          <div class="flex h-full min-h-48 items-center justify-center p-6 text-xs text-surface-500">
            This file is not plain text, so Medousa cannot edit it here.
          </div>
        {/if}
      </div>
      {#if secondaryTab && context?.worktree}
        <CodeSplitEditorPane
          tab={secondaryTab}
          worktree={context.worktree}
          leaseId={context.leaseId}
          generation={context.leaseGeneration}
          onFocus={() => (focusedSide = true)}
          onClose={() => codeWorkspace.closeSide(workId)}
        />
      {/if}
    </div>

    {#if contextPanel}
      <div class="max-h-44 shrink-0 overflow-y-auto border-t border-surface-500/30 bg-surface-950/80">
        <div class="sticky top-0 z-10 flex items-center justify-between border-b border-surface-500/25 bg-surface-950 px-2 py-1">
          <span class="text-[9px] font-medium uppercase tracking-wider text-surface-400">
            {contextPanel === "problems" ? "Issues" : "Structure"}
          </span>
          <button type="button" class="rounded p-0.5 text-surface-500 hover:text-surface-200" aria-label="Close context panel" onclick={() => (contextPanel = null)}><X size={11} /></button>
        </div>
        {#if contextPanel === "problems"}
          {#if problems.length === 0}
            <p class="px-3 py-3 text-[10px] text-surface-500">No issues found in this file.</p>
          {:else}
            {#each problems as problem, index (`${problem.from}:${problem.message}:${index}`)}
              <button
                type="button"
                class="flex w-full items-start gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
                onclick={() => editor?.revealLine(problem.line)}
              >
                <CircleAlert size={11} class={problem.severity === "error" ? "mt-0.5 shrink-0 text-rose-300" : "mt-0.5 shrink-0 text-amber-300"} />
                <span class="min-w-0 flex-1 text-[10px] text-surface-300">{problem.message}</span>
                <span class="shrink-0 font-mono text-[9px] text-surface-500">{problem.line}</span>
              </button>
            {/each}
          {/if}
        {:else if symbolsLoading}
          <p class="px-3 py-3 text-[10px] text-surface-500">Reading file structure…</p>
        {:else if symbols.length === 0}
          <p class="px-3 py-3 text-[10px] text-surface-500">No structure is available for this file.</p>
        {:else}
          {#each symbols as symbol (`${symbol.name}:${symbolLine(symbol)}`)}
            <button
              type="button"
              class="flex w-full items-center gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
              onclick={() => editor?.revealLine(symbolLine(symbol))}
            >
              <ListTree size={11} class="shrink-0 text-primary-300/70" />
              <span class="min-w-0 flex-1 truncate text-[10px] text-surface-300">{symbol.name}</span>
              <span class="font-mono text-[9px] text-surface-500">{symbolLine(symbol)}</span>
            </button>
          {/each}
        {/if}
      </div>
    {/if}

    <footer class="flex shrink-0 items-center justify-between gap-2 overflow-x-auto border-t border-surface-500/25 bg-surface-950/75 px-1.5 py-0.5">
      <div class="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-35"
          disabled={!terminalAvailable}
          onclick={onOpenTerminal}
        ><SquareTerminal size={10} />Run</button>
        <button
          type="button"
          class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200"
          class:bg-surface-800={worldOpen}
          onclick={onToggleWorld}
        ><Orbit size={10} />Understand</button>
        {#if reviewAvailable}
          <button
            type="button"
            class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-amber-300/80 hover:bg-surface-800 hover:text-amber-200"
            onclick={onOpenReview}
          ><GitPullRequestArrow size={10} />Review</button>
        {/if}
      </div>
      <div class="flex shrink-0 items-center gap-0.5">
        <button
        type="button"
        class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200"
        class:bg-surface-800={contextPanel === "problems"}
        onclick={() => (contextPanel = contextPanel === "problems" ? null : "problems")}
      ><CircleAlert size={10} />{problems.length}</button>
      <button
        type="button"
        class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-35"
        class:bg-surface-800={contextPanel === "outline"}
        disabled={!lspClient}
        onclick={() => void showOutline()}
      ><ListTree size={10} />Structure</button>
      </div>
    </footer>
  {:else}
    <div class="flex min-h-72 flex-1 items-center justify-center p-8 text-center">
      <div class="max-w-xs">
        <FileCode2 size={24} class="mx-auto text-surface-600" />
        <p class="mt-2 text-xs font-medium text-surface-300">Choose a file</p>
        <p class="mt-1 text-[10px] leading-relaxed text-surface-500">
          Pick one from the project files. Medousa will remember your open files, drafts, and place while you move through the rest of your workspace.
        </p>
      </div>
    </div>
  {/if}
</section>

<svelte:window
  onkeydown={(event) => {
    const command = event.metaKey || event.ctrlKey;
    if (command && event.shiftKey && event.key.toLowerCase() === "s") {
      event.preventDefault();
      void saveAll();
      return;
    }
    if (command && event.key.toLowerCase() === "s") {
      event.preventDefault();
      const target = focusedSide && secondaryTab ? secondaryTab : activeTab;
      if (target && codeWorkspace.isDirty(target)) void saveTab(target);
      return;
    }
    if (command && event.key.toLowerCase() === "w" && activeTab) {
      event.preventDefault();
      close(activeTab);
      return;
    }
    if (event.ctrlKey && event.key === "Tab") {
      event.preventDefault();
      cycleTab(event.shiftKey ? -1 : 1);
      return;
    }
    if (command && event.key === "\\") {
      event.preventDefault();
      toggleSplit();
      return;
    }
    if (
      (event.metaKey || event.ctrlKey) &&
      event.shiftKey &&
      event.key.toLowerCase() === "o" &&
      activeTab
    ) {
      event.preventDefault();
      void showOutline();
    }
    if (event.key === "Escape" && contextPanel) contextPanel = null;
  }}
/>
