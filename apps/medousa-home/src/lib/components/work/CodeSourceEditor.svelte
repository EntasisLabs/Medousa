<script lang="ts">
  import { tick, untrack } from "svelte";
  import {
    CircleAlert,
    ArrowLeft,
    ArrowRight,
    Columns2,
    FileCode2,
    ListTree,
    LoaderCircle,
    RotateCcw,
    Save,
    SquareTerminal,
    GitPullRequestArrow,
    Orbit,
    Sparkles,
    X,
    Search,
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
    getUndertakingSourceTree,
    type ForgeSourceTreeFile,
    getProjectTasks,
    runProjectTask,
    type ProjectTask,
    type ProjectTaskResult,
    type ForgeSourceFile,
  } from "$lib/forge";
  import { codeWorkspace, type CodeDocumentTab } from "$lib/stores/codeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { setActiveCodeInsights } from "$lib/utils/undertakingWorkspace";

  interface Props {
    fill?: boolean;
    worldOpen?: boolean;
    reviewAvailable?: boolean;
    terminalAvailable?: boolean;
    onToggleWorld?: () => void;
    onOpenReview?: () => void;
    onOpenTerminal?: () => void;
    preferredAgent?: "codex" | "cursor";
    onHandoffToAgent?: (runtime: "codex" | "cursor", draft?: string) => Promise<void>;
    onReclaimHuman?: () => Promise<void>;
  }

  let {
    fill = false,
    worldOpen = false,
    reviewAvailable = false,
    terminalAvailable = false,
    onToggleWorld,
    onOpenReview,
    onOpenTerminal,
    preferredAgent = "codex",
    onHandoffToAgent,
    onReclaimHuman,
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
  let quickOpen = $state(false);
  let quickQuery = $state("");
  let quickFiles = $state<ForgeSourceTreeFile[]>([]);
  let quickLoading = $state(false);
  let quickIndex = $state(0);
  let projectTasks = $state<ProjectTask[]>([]);
  let selectedTaskId = $state("");
  let runningTask = $state(false);
  let taskResult = $state<ProjectTaskResult | null>(null);
  let externalVersions = $state<Record<string, ForgeSourceFile>>({});
  let comparingTabId = $state<string | null>(null);
  let quickInput = $state<HTMLInputElement | null>(null);

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
  const agentHasControl = $derived(
    Boolean(context?.executorKind && context.executorKind !== "human"),
  );
  const documentUri = $derived.by(() => {
    if (!activeTab || !context?.worktree) return null;
    return pathToFileUri(
      `${context.worktree.replace(/[\\/]$/, "")}/${activeTab.path}`,
    );
  });
  const quickResults = $derived.by(() => {
    const needle = quickQuery.trim().toLowerCase();
    const scored = quickFiles.map((file, index) => {
      const path = file.path.toLowerCase();
      const name = path.split("/").pop() ?? path;
      const score = !needle ? index : name.startsWith(needle) ? 0 : name.includes(needle) ? 1 : path.includes(needle) ? 2 : 99;
      return { file, score };
    });
    return scored.filter((row) => row.score < 99).sort((a, b) => a.score - b.score).slice(0, 80).map((row) => row.file);
  });
  const selectedTask = $derived(
    projectTasks.find((task) => task.id === selectedTaskId) ?? projectTasks[0] ?? null,
  );

  async function runDetectedTask() {
    if (!selectedTask || runningTask) {
      onOpenTerminal?.();
      return;
    }
    runningTask = true;
    surfaceError = null;
    try {
      let leaseId = context?.leaseId ?? null;
      let generation = context?.leaseGeneration ?? null;
      if ((!leaseId || generation == null) && detail?.allowed_actions.begin_attempt.allowed) {
        const begun = await beginHumanAttempt(detail.id);
        leaseId = begun.lease.lease_id;
        generation = begun.lease.generation;
        undertakings.setActiveFromItem(begun.item, {
          leaseId,
          leaseGeneration: generation,
          executorKind: "human",
        });
      }
      if (!leaseId || generation == null) throw new Error("This project is not ready to run checks");
      taskResult = await runProjectTask(workId, selectedTask.id, {
        lease_id: leaseId,
        generation,
      });
      await undertakings.refreshDetail();
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      runningTask = false;
    }
  }

  async function showQuickOpen() {
    quickOpen = true;
    quickQuery = "";
    quickIndex = 0;
    await tick();
    quickInput?.focus();
    if (quickFiles.length || !workId) return;
    quickLoading = true;
    try {
      quickFiles = (await getUndertakingSourceTree(workId)).files;
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      quickLoading = false;
    }
  }

  async function chooseQuickFile(file = quickResults[quickIndex]) {
    if (!file) return;
    quickOpen = false;
    const tab = await codeWorkspace.open(workId, file.path, 1);
    undertakings.setSelection({ path: file.path, line: 1, entityId: null });
    await tick();
    if (tab) editor?.focusEditor();
  }

  async function navigate(direction: -1 | 1) {
    const tab = await codeWorkspace.navigate(workId, direction);
    if (!tab) return;
    undertakings.setSelection({ path: tab.path, line: tab.line, entityId: null });
    await tick();
    if (tab.line) editor?.revealLine(tab.line);
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "p") {
      event.preventDefault();
      void showQuickOpen();
    }
    if (event.key === "Escape" && quickOpen) quickOpen = false;
  }

  function openLanguagePackages() {
    settingsNav.openSection("packages");
    shellTabs.openDestination("settings");
    layout.openShellSidebarView("settings");
  }

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
        externalVersions = { ...externalVersions, [current.tabId]: source };
        codeWorkspace.setError(
          current.tabId,
          "The project version changed while you were editing. Your draft is safe.",
        );
      } else {
        codeWorkspace.acceptSaved(current.tabId, source);
      }
    } catch {
      // Tree refresh and explicit reload surface durable errors. Polling stays quiet.
    }
  }

  function useProjectVersion(tab: CodeDocumentTab) {
    const source = externalVersions[tab.tabId];
    if (!source) return;
    codeWorkspace.acceptSaved(tab.tabId, source);
    const next = { ...externalVersions };
    delete next[tab.tabId];
    externalVersions = next;
    comparingTabId = null;
  }

  function keepDraft(tab: CodeDocumentTab) {
    const source = externalVersions[tab.tabId];
    if (!source) return;
    codeWorkspace.rebaseDraft(tab.tabId, source);
    const next = { ...externalVersions };
    delete next[tab.tabId];
    externalVersions = next;
    comparingTabId = null;
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

  async function saveTab(tab: CodeDocumentTab | null): Promise<boolean> {
    const active = context;
    if (
      !tab ||
      !active?.leaseId ||
      active.leaseGeneration == null ||
      !codeWorkspace.isDirty(tab) ||
      saving
    ) return !tab || !codeWorkspace.isDirty(tab);
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
      return true;
    } catch (err) {
      codeWorkspace.setError(
        tab.tabId,
        err instanceof Error ? err.message : String(err),
      );
      return false;
    } finally {
      saving = false;
    }
  }

  async function save() {
    await saveTab(activeTab);
  }

  async function saveAll(): Promise<boolean> {
    for (const tab of tabs) {
      if (codeWorkspace.isDirty(tab) && !(await saveTab(tab))) return false;
    }
    return true;
  }

  async function handoffToAgent(draft?: string) {
    if (!onHandoffToAgent || saving) return;
    surfaceError = null;
    if (!(await saveAll())) {
      surfaceError = "Resolve the unsaved file before asking an agent to continue.";
      return;
    }
    saving = true;
    try {
      await onHandoffToAgent(preferredAgent, draft);
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function reclaimHuman() {
    if (!onReclaimHuman || saving) return;
    saving = true;
    surfaceError = null;
    try {
      await onReclaimHuman();
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function cycleTab(direction: 1 | -1) {
    if (tabs.length < 2 || !activeTab) return;
    const recent = codeWorkspace.recentTabsFor(workId);
    const index = recent.findIndex((tab) => tab.tabId === activeTab.tabId);
    const next = recent[(index + direction + recent.length) % recent.length];
    if (next) activate(next);
  }

  function tabLabel(tab: CodeDocumentTab): string {
    if (tabs.filter((entry) => entry.title === tab.title).length < 2) return tab.title;
    const parts = tab.path.split("/");
    return parts.slice(-2).join("/");
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

  function containingSymbol(): string | null {
    const line = context?.selectionStartLine ?? context?.selectedLine;
    if (!line) return null;
    const containing = symbols
      .filter((symbol) => line >= symbolLine(symbol))
      .sort((a, b) => symbolLine(b) - symbolLine(a))[0];
    return containing?.name ?? null;
  }

  $effect(() => {
    if (!workId) return;
    setActiveCodeInsights(workId, {
      containing_symbol: containingSymbol(),
      diagnostics: problems.slice(0, 20).map((problem) =>
        `${activeTab?.path ?? "current file"}:${problem.line} ${problem.message}`
      ),
      last_verification: taskResult
        ? `${taskResult.task.label}: ${taskResult.success ? "passed" : "failed"}${taskResult.exit_code != null ? ` (exit ${taskResult.exit_code})` : ""}`
        : null,
    });
  });

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
    const id = workId;
    const prepared = Boolean(context?.worktree);
    if (!id || !prepared) {
      projectTasks = [];
      selectedTaskId = "";
      return;
    }
    let cancelled = false;
    void getProjectTasks(id).then((tasks) => {
      if (cancelled) return;
      projectTasks = tasks;
      selectedTaskId = tasks.find((task) => task.kind === "verify")?.id ?? tasks[0]?.id ?? "";
    }).catch(() => {
      if (!cancelled) projectTasks = [];
    });
    return () => { cancelled = true; };
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
            <span class="truncate">{tabLabel(tab)}</span>
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
        <div class="flex shrink-0 items-center">
          <button type="button" class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-25" aria-label="Go back" title="Go back" disabled={!codeWorkspace.canNavigate(workId, -1)} onclick={() => void navigate(-1)}><ArrowLeft size={11} /></button>
          <button type="button" class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-25" aria-label="Go forward" title="Go forward" disabled={!codeWorkspace.canNavigate(workId, 1)} onclick={() => void navigate(1)}><ArrowRight size={11} /></button>
        </div>
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
        {#if agentHasControl}
          <span class="hidden text-[9px] text-primary-200 sm:inline">
            {context?.executorKind === "cursor" ? "Cursor" : "Codex"} has the project
          </span>
          <button
            type="button"
            class="rounded bg-primary-500/80 px-2 py-1 text-[10px] font-medium text-surface-50 disabled:opacity-40"
            disabled={saving}
            onclick={() => void reclaimHuman()}
          >Resume editing</button>
        {:else if !editable && detail?.allowed_actions.begin_attempt.allowed}
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
    {#if context?.selectedText && onHandoffToAgent && !agentHasControl}
      <div class="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-primary-500/20 bg-primary-950/15 px-2 py-1" aria-label="Selected code actions">
        <span class="mr-1 flex shrink-0 items-center gap-1 text-[9px] text-primary-200/80"><Sparkles size={10} />Selection</span>
        <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Help me understand the selected code and answer my questions about it.")}>Ask</button>
        <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Change the selected code. Ask only if the intended change is ambiguous.")}>Change</button>
        {#if problems.length > 0}
          <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Fix the relevant issue in the selected code and verify the result.")}>Fix</button>
        {/if}
        <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Explain the selected code clearly, including its role and important behavior.")}>Explain</button>
        <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Add the most valuable focused test for the selected code and run the relevant check.")}>Add test</button>
      </div>
    {/if}
    {#if externalVersions[activeTab.tabId]}
      <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-amber-500/30 bg-amber-950/20 px-2.5 py-1.5 text-[10px] text-amber-100">
        <span class="min-w-40 flex-1">This file changed elsewhere. Your draft is safe.</span>
        <button type="button" class="rounded px-1.5 py-0.5 hover:bg-white/10" onclick={() => (comparingTabId = activeTab.tabId)}>Compare</button>
        <button type="button" class="rounded px-1.5 py-0.5 hover:bg-white/10" onclick={() => useProjectVersion(activeTab)}>Use project version</button>
        <button type="button" class="rounded bg-amber-500/20 px-1.5 py-0.5" onclick={() => keepDraft(activeTab)}>Keep my draft</button>
      </div>
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
              onCursorChanged={(line) => {
                codeWorkspace.updateLine(activeTab.tabId, line);
                undertakings.setSelection({ path: activeTab.path, line });
              }}
              onSelectionChanged={(selection) =>
                undertakings.setSelection({
                  path: activeTab.path,
                  line: selection.startLine,
                  selectionStartLine: selection.startLine,
                  selectionEndLine: selection.endLine,
                  selectedText: selection.text || null,
                })}
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
        {#if lspError}
          <button type="button" class="rounded px-1.5 py-0.5 text-[9px] text-amber-300/80 hover:bg-surface-800 hover:text-amber-200" title={lspError} onclick={openLanguagePackages}>Add language support</button>
        {/if}
        <button
          type="button"
          class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-35"
          disabled={!terminalAvailable && !selectedTask}
          onclick={() => void runDetectedTask()}
          title={selectedTask ? selectedTask.argv.join(" ") : "Open Terminal"}
        >{#if runningTask}<LoaderCircle size={10} class="animate-spin" />{:else}<SquareTerminal size={10} />{/if}{selectedTask?.label ?? "Terminal"}</button>
        {#if projectTasks.length > 1}
          <select class="max-w-24 rounded bg-transparent py-0.5 text-[9px] text-surface-500 outline-none" aria-label="Project command" bind:value={selectedTaskId}>
            {#each projectTasks as task (task.id)}
              <option value={task.id}>{task.label}</option>
            {/each}
          </select>
        {/if}
        {#if selectedTask && terminalAvailable}
          <button type="button" class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200" onclick={onOpenTerminal} title="Open Terminal"><SquareTerminal size={10} />Terminal</button>
        {/if}
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
        <span class="mr-1 text-[9px] text-surface-500">
          {#if agentHasControl}
            {context?.executorKind === "cursor" ? "Cursor" : "Codex"} is working
          {:else if editable && context?.boundTerminalSessionIds.length}
            You + Terminal
          {:else if editable}
            You are editing
          {:else}
            Ready
          {/if}
        </span>
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
    {#if taskResult}
      <button type="button" class="flex shrink-0 items-center justify-between gap-2 border-t px-2.5 py-1 text-left text-[9px] {taskResult.success ? 'border-emerald-500/25 bg-emerald-950/20 text-emerald-200' : 'border-rose-500/30 bg-rose-950/25 text-rose-200'}" title="Open Terminal for detailed investigation" onclick={onOpenTerminal}>
        <span>{taskResult.success ? "Passed" : "Needs attention"} · {taskResult.task.label}</span>
        <span class="text-current opacity-60">{(taskResult.duration_ms / 1000).toFixed(1)}s{taskResult.exit_code != null ? ` · exit ${taskResult.exit_code}` : ""}</span>
      </button>
    {/if}
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

<style>
  .code-intent-action {
    border-radius: 0.25rem;
    padding: 0.2rem 0.45rem;
    color: rgb(var(--color-surface-300));
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

{#if quickOpen}
  <div class="fixed inset-0 z-[120] flex items-start justify-center px-4 pt-[12vh]">
    <button type="button" class="absolute inset-0 bg-black/35" aria-label="Close file picker" onclick={() => (quickOpen = false)}></button>
    <div class="relative w-full max-w-xl overflow-hidden rounded-lg border border-surface-500/50 bg-surface-950 shadow-2xl" role="dialog" aria-modal="true" aria-label="Open a file" tabindex="-1">
      <div class="flex items-center gap-2 border-b border-surface-500/30 px-3">
        <Search size={14} class="text-surface-500" />
        <input bind:this={quickInput} class="min-w-0 flex-1 bg-transparent py-2.5 text-sm text-surface-100 outline-none" placeholder="Open a file" bind:value={quickQuery} oninput={() => (quickIndex = 0)} onkeydown={(event) => {
          if (event.key === "ArrowDown") { event.preventDefault(); quickIndex = Math.min(quickIndex + 1, quickResults.length - 1); }
          if (event.key === "ArrowUp") { event.preventDefault(); quickIndex = Math.max(quickIndex - 1, 0); }
          if (event.key === "Enter") { event.preventDefault(); void chooseQuickFile(); }
        }} />
        <span class="text-[9px] text-surface-600">⌘P</span>
      </div>
      <div class="max-h-[50vh] overflow-y-auto py-1">
        {#if quickLoading}
          <p class="px-3 py-3 text-xs text-surface-500">Reading project files…</p>
        {:else if quickResults.length === 0}
          <p class="px-3 py-3 text-xs text-surface-500">No matching files.</p>
        {:else}
          {#each quickResults as file, index (file.path)}
            <button type="button" class="flex w-full items-center gap-2 px-3 py-1.5 text-left {index === quickIndex ? 'bg-surface-800 text-surface-100' : 'text-surface-400 hover:bg-surface-900'}" onmouseenter={() => (quickIndex = index)} onclick={() => void chooseQuickFile(file)}>
              <FileCode2 size={12} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-xs">{file.path.split("/").pop()}</span>
              <span class="min-w-0 max-w-[60%] truncate font-mono text-[9px] text-surface-600">{file.path}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if comparingTabId}
  {@const conflictTab = tabs.find((tab) => tab.tabId === comparingTabId)}
  {@const projectVersion = externalVersions[comparingTabId]}
  {#if conflictTab && projectVersion}
    <div class="fixed inset-0 z-[125] flex items-center justify-center p-4">
      <button type="button" class="absolute inset-0 bg-black/55" aria-label="Close comparison" onclick={() => (comparingTabId = null)}></button>
      <div class="relative flex max-h-[85vh] w-full max-w-5xl flex-col overflow-hidden rounded-lg border border-surface-500/50 bg-surface-950 shadow-2xl" role="dialog" aria-modal="true" aria-label="Compare file versions" tabindex="-1">
        <header class="flex items-center justify-between border-b border-surface-500/30 px-3 py-2">
          <div><p class="text-xs font-medium text-surface-100">Choose the version to continue with</p><p class="font-mono text-[9px] text-surface-500">{conflictTab.path}</p></div>
          <button type="button" class="rounded p-1 text-surface-500 hover:text-surface-100" aria-label="Close comparison" onclick={() => (comparingTabId = null)}><X size={13} /></button>
        </header>
        <div class="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-2">
          <section class="flex min-h-48 flex-col border-b border-surface-500/30 md:border-b-0 md:border-r"><p class="border-b border-surface-500/25 px-3 py-1.5 text-[10px] font-medium text-surface-300">My draft</p><pre class="min-h-0 flex-1 overflow-auto p-3 text-[10px] leading-relaxed text-surface-300">{conflictTab.draft}</pre></section>
          <section class="flex min-h-48 flex-col"><p class="border-b border-surface-500/25 px-3 py-1.5 text-[10px] font-medium text-surface-300">Project version</p><pre class="min-h-0 flex-1 overflow-auto p-3 text-[10px] leading-relaxed text-surface-300">{projectVersion.content}</pre></section>
        </div>
        <footer class="flex justify-end gap-2 border-t border-surface-500/30 px-3 py-2"><button type="button" class="rounded px-2 py-1 text-[10px] text-surface-300 hover:bg-surface-800" onclick={() => useProjectVersion(conflictTab)}>Use project version</button><button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-[10px] font-medium text-white" onclick={() => keepDraft(conflictTab)}>Keep my draft</button></footer>
      </div>
    </div>
  {/if}
{/if}

<svelte:window
  onkeydown={(event) => {
    onWindowKeydown(event);
    if (event.defaultPrevented) return;
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
