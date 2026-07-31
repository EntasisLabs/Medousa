<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
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
  import CodeBreadcrumbs from "$lib/components/code/CodeBreadcrumbs.svelte";
  import CodeDocumentTabStrip from "$lib/components/code/CodeDocumentTabStrip.svelte";
  import CodeEditorContextMenu, {
    type CodeEditorMenuAction,
  } from "$lib/components/code/CodeEditorContextMenu.svelte";
  import CodeSplitEditorPane from "$lib/components/work/CodeSplitEditorPane.svelte";
  import CodeTerminalDock from "$lib/components/work/CodeTerminalDock.svelte";
  import { openTrackedTerminal } from "$lib/utils/undertakingWorkspace";
  import type { LSPClient } from "@codemirror/lsp-client";
  import {
    getCodeWorkspaceLspClient,
    getCodeDocumentSymbols,
    getCodeWorkspaceDiagnostics,
    getCodeWorkspaceSymbols,
    getCodeLanguageCapabilities,
    getCodeEditorConventions,
    requestCodeLanguageAction,
    pathToFileUri,
    type CodeDocumentSymbol,
    type CodeWorkspaceDiagnostic,
    type CodeWorkspaceSymbol,
  } from "$lib/code/codingEngineClient";
  import { containingSymbolTrail } from "$lib/code/codeDocumentSymbols";
  import { languageSupportsLsp } from "$lib/code/codeEditorLanguageRegistry";
  import {
    beginHumanAttempt,
    getUndertakingSource,
    heartbeatLease,
    humanizeForgeMessage,
    saveUndertakingSource,
    saveUndertakingSources,
    getUndertakingSourceTree,
    type ForgeSourceTreeFile,
    getProjectTasks,
    getProjectTests,
    startProjectTaskRun,
    getProjectTaskRun,
    cancelProjectTaskRun,
    getReviewFile,
    type ProjectTask,
    type ProjectTaskResult,
    type ProjectTaskRun,
    type ProjectTest,
    type ForgeSourceFile,
  } from "$lib/forge";
  import { codeWorkspace, type CodeDocumentTab } from "$lib/stores/codeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { setActiveCodeInsights } from "$lib/utils/undertakingWorkspace";
  import { deferCodeWorkspaceWork } from "$lib/utils/codeWorkspaceTrace";
  import { fetchPackagesCatalog, installPackage } from "$lib/utils/packagesApi";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { formatShortcut } from "$lib/platform";
  import {
    readCodeEditorLineNumbers,
    readCodeEditorWordWrap,
    writeCodeEditorLineNumbers,
    writeCodeEditorWordWrap,
  } from "$lib/config/codeEditorPreferences";
  import { codeEditorFind } from "$lib/stores/codeEditorFind.svelte";

  interface Props {
    fill?: boolean;
    worldOpen?: boolean;
    reviewAvailable?: boolean;
    terminalAvailable?: boolean;
    onToggleWorld?: () => void;
    onOpenReview?: () => void;
    onOpenTerminal?: () => void;
    onProvision?: () => Promise<void>;
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
    onProvision,
    preferredAgent = "codex",
    onHandoffToAgent,
    onReclaimHuman,
  }: Props = $props();

  let saving = $state(false);
  let saveWhisper = $state<string | null>(null);
  let saveWhisperTimer: ReturnType<typeof setTimeout> | null = null;
  let surfaceError = $state<string | null>(null);
  let wordWrap = $state(readCodeEditorWordWrap());
  let showLineNumbers = $state(readCodeEditorLineNumbers());
  let findOpenByTabId = $state<Record<string, boolean>>({});
  let terminalDockOpen = $state(false);
  let dockSessionId = $state<string | null>(null);
  let dockBusy = $state(false);
  let editor = $state<CodeMirrorHost | undefined>();
  let lspClient = $state<LSPClient | null>(null);
  let lspError = $state<string | null>(null);
  let lspConnecting = $state(false);
  let contextPanel = $state<"problems" | "outline" | "references" | null>(null);
  let problems = $state<ReturnType<CodeMirrorHost["getProblems"]>>([]);
  let symbols = $state<CodeDocumentSymbol[]>([]);
  let symbolsLoading = $state(false);
  let focusedSide = $state(false);
  let quickOpen = $state(false);
  let quickQuery = $state("");
  let quickFiles = $state<ForgeSourceTreeFile[]>([]);
  let quickSymbols = $state<CodeWorkspaceSymbol[]>([]);
  let quickSymbolQuery = $state("");
  let quickLoading = $state(false);
  let quickIndex = $state(0);
  let projectTasks = $state<ProjectTask[]>([]);
  let selectedTaskId = $state("");
  let runningTask = $state(false);
  let taskResult = $state<ProjectTaskResult | null>(null);
  let taskRun = $state<ProjectTaskRun | null>(null);
  let projectTests = $state<ProjectTest[]>([]);
  let testsOpen = $state(false);
  let externalVersions = $state<Record<string, ForgeSourceFile>>({});
  let comparingTabId = $state<string | null>(null);
  let reviewChangedLines = $state<Array<{ line: number; kind: string }>>([]);
  let quickInput = $state<HTMLInputElement | null>(null);
  let workspaceDiagnostics = $state<CodeWorkspaceDiagnostic[]>([]);
  let languageCapabilities = $state<Record<string, unknown>>({});
  let editorConventions = $state<{ indent_style?: "space" | "tab"; indent_size?: string; tab_width?: string }>({});
  let references = $state<Array<{ uri?: string; range?: { start?: { line?: number } } }>>([]);
  let languageActionRunning = $state(false);
  let repairingLanguage = $state(false);
  let lspRetry = $state(0);
  let cursorLine = $state(1);
  let cursorColumn = $state(1);
  let editorSelection = $state<{
    startLine: number;
    endLine: number;
    text: string;
  } | null>(null);
  let linePersistTimer: ReturnType<typeof setTimeout> | null = null;
  let editorMenuOpen = $state(false);
  let editorMenuX = $state(0);
  let editorMenuY = $state(0);
  let renameOpen = $state(false);
  let renameDraft = $state("");
  let renameInput = $state<HTMLInputElement | null>(null);

  const context = $derived(undertakings.active);
  const detail = $derived(undertakings.detail);
  const workId = $derived(detail?.id ?? context?.workId ?? "");
  const tabs = $derived(workId ? codeWorkspace.orderedTabsFor(workId) : []);
  const activeTab = $derived.by(() => {
    const activeId = codeWorkspace.activeByWorkId[workId];
    return activeId
      ? (codeWorkspace.tabs.find((tab) => tab.tabId === activeId) ?? null)
      : null;
  });
  // Effects that start network/language-service work must depend on tab
  // identity, not the mutable tab snapshot (cursor line, draft, diagnostics).
  const activeTabId = $derived(activeTab?.tabId ?? "");
  const activeTabPath = $derived(activeTab?.path ?? "");
  const activeTabLanguage = $derived(activeTab?.language ?? "");
  const activeTabLine = $derived(activeTab?.line ?? null);
  const dirty = $derived(Boolean(activeTab && codeWorkspace.isDirty(activeTab)));
  const dirtyCount = $derived(tabs.filter((tab) => codeWorkspace.isDirty(tab)).length);
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
  const operatorLabel = $derived.by(() => {
    if (agentHasControl) {
      return context?.executorKind === "cursor" ? "Cursor editing" : "Codex editing";
    }
    if (editable) return "You editing";
    return "Ready";
  });
  const lastVerifyLabel = $derived.by(() => {
    if (!taskResult) return null;
    return `${taskResult.success ? "Passed" : "Failed"} · ${taskResult.task.label}`;
  });
  const landError = $derived(workId ? codeWorkspace.workspaceErrorByWorkId[workId] ?? null : null);
  const needsProvision = $derived(
    Boolean(detail && !detail.environment && detail.allowed_actions.provision.allowed),
  );
  const documentUri = $derived.by(() => {
    if (!activeTabPath || !context?.worktree) return null;
    return pathToFileUri(
      `${context.worktree.replace(/[\\/]$/, "")}/${activeTabPath}`,
    );
  });
  const quickResults = $derived.by(() => {
    const needle = quickQuery.trim().replace(/^>/, "").toLowerCase();
    const scored = quickFiles.map((file, index) => {
      const path = file.path.toLowerCase();
      const name = path.split("/").pop() ?? path;
      const score = !needle ? index : name.startsWith(needle) ? 0 : name.includes(needle) ? 1 : path.includes(needle) ? 2 : 99;
      return { file, score };
    });
    return scored.filter((row) => row.score < 99).sort((a, b) => a.score - b.score).slice(0, 80).map((row) => row.file);
  });
  const quickMode = $derived(
    quickQuery.startsWith("@") ? "symbol" : quickQuery.startsWith(":") ? "line" : "file",
  );
  const quickSymbolResults = $derived(
    quickSymbols.slice(0, 80),
  );
  const quickResultCount = $derived(
    quickMode === "symbol" ? quickSymbolResults.length : quickMode === "line" ? 1 : quickResults.length,
  );
  const selectedTask = $derived(
    projectTasks.find((task) => task.id === selectedTaskId) ?? projectTasks[0] ?? null,
  );
  const workspaceProblemRows = $derived.by(() => {
    const rows = workspaceDiagnostics.flatMap((document) => {
      const path = pathFromUri(document.uri) ?? document.uri ?? "Project";
      return (document.diagnostics ?? []).map((diagnostic) => ({
        path,
        line: (diagnostic.range?.start?.line ?? 0) + 1,
        message: diagnostic.message ?? "Language issue",
        severity: diagnostic.severity ?? 2,
      }));
    });
    return rows.length > 0
      ? rows
      : problems.map((problem) => ({
          path: activeTab?.path ?? "Current file",
          line: problem.line,
          message: problem.message,
          severity: problem.severity === "error" ? 1 : 2,
        }));
  });
  const canReference = $derived(Boolean(languageCapabilities.referencesProvider));
  const canRename = $derived(Boolean(languageCapabilities.renameProvider));
  const canFormat = $derived(Boolean(languageCapabilities.documentFormattingProvider));
  const canCodeAction = $derived(Boolean(languageCapabilities.codeActionProvider));
  const canDefinition = $derived(Boolean(languageCapabilities.definitionProvider ?? lspClient));
  const symbolTrail = $derived(
    containingSymbolTrail(
      symbols,
      context?.selectionStartLine ?? context?.selectedLine ?? cursorLine,
    ),
  );
  const indentStatusLabel = $derived.by(() => {
    const size =
      Number.parseInt(editorConventions.indent_size ?? editorConventions.tab_width ?? "", 10) ||
      2;
    return editorConventions.indent_style === "tab"
      ? `Tab Size: ${size}`
      : `Spaces: ${size}`;
  });
  const absolutePath = $derived.by(() => {
    if (!activeTab || !context?.worktree) return activeTab?.path ?? "";
    return `${context.worktree.replace(/[\\/]$/, "")}/${activeTab.path}`;
  });

  async function runDetectedTask(test?: ProjectTest) {
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
      taskRun = await startProjectTaskRun(workId, test?.task_id ?? selectedTask.id, {
        lease_id: leaseId,
        generation,
        test_id: test?.id,
      });
      while (taskRun.state === "running") {
        await new Promise((resolve) => setTimeout(resolve, 350));
        taskRun = await getProjectTaskRun(workId, taskRun.run_id);
      }
      taskResult = taskRun.result ?? null;
      await undertakings.refreshDetail();
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      runningTask = false;
    }
  }

  async function stopDetectedTask() {
    if (!taskRun || taskRun.state !== "running") return;
    try {
      taskRun = await cancelProjectTaskRun(workId, taskRun.run_id);
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    }
  }

  async function toggleTests() {
    testsOpen = !testsOpen;
    if (!testsOpen || projectTests.length || !workId) return;
    try {
      projectTests = await getProjectTests(workId);
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    }
  }

  async function openTaskLocation(path: string, line: number) {
    await codeWorkspace.open(workId, path, line);
    undertakings.setSelection({ path, line, entityId: null });
    await tick();
    editor?.revealLine(line);
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

  function toggleWordWrap() {
    wordWrap = !wordWrap;
    writeCodeEditorWordWrap(wordWrap);
  }

  function toggleLineNumbers() {
    showLineNumbers = !showLineNumbers;
    writeCodeEditorLineNumbers(showLineNumbers);
  }

  function handleCursorChanged(tab: CodeDocumentTab, cursor: { line: number; column: number }) {
    cursorLine = cursor.line;
    cursorColumn = cursor.column;
    if (linePersistTimer) clearTimeout(linePersistTimer);
    linePersistTimer = setTimeout(() => {
      linePersistTimer = null;
      codeWorkspace.updateLine(tab.tabId, cursor.line);
    }, 500);
  }

  function captureEditorContext() {
    if (!activeTab) return;
    undertakings.setSelection({
      path: activeTab.path,
      line: editorSelection?.startLine ?? cursorLine,
      selectionStartLine: editorSelection?.startLine ?? null,
      selectionEndLine: editorSelection?.endLine ?? null,
      selectedText: editorSelection?.text || null,
      entityId: null,
    });
    setActiveCodeInsights(workId, {
      containing_symbol: containingSymbol(),
      diagnostics: problems.slice(0, 20).map((problem) =>
        `${activeTab.path}:${problem.line} ${problem.message}`
      ),
      last_verification: taskResult
        ? `${taskResult.task.label}: ${taskResult.success ? "passed" : "failed"}${taskResult.exit_code != null ? ` (exit ${taskResult.exit_code})` : ""}`
        : null,
    });
  }

  async function toggleTerminalDock(forceOpen?: boolean) {
    const next = forceOpen === true ? true : forceOpen === false ? false : !terminalDockOpen;
    if (!next) {
      terminalDockOpen = false;
      return;
    }
    if (!detail || !terminalAvailable) {
      surfaceError = "Terminal is not available for this project yet.";
      return;
    }
    terminalDockOpen = true;
    if (dockSessionId) return;
    dockBusy = true;
    surfaceError = null;
    try {
      const sessionId = await openTrackedTerminal(detail, { activate: false });
      dockSessionId = sessionId;
      if (!sessionId) surfaceError = "Could not open a workshop shell for this project.";
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
      terminalDockOpen = false;
    } finally {
      dockBusy = false;
    }
  }

  async function popOutTerminal() {
    if (!detail) return;
    terminalDockOpen = false;
    await openTrackedTerminal(detail, { activate: true });
  }

  async function chooseQuickFile(file = quickResults[quickIndex]) {
    if (!file) return;
    quickOpen = false;
    const tab = await codeWorkspace.open(workId, file.path, 1);
    undertakings.setSelection({ path: file.path, line: 1, entityId: null });
    await tick();
    if (tab) editor?.focusEditor();
  }

  function pathFromUri(uri = ""): string | null {
    if (!uri || !context?.worktree) return null;
    const decoded = decodeURIComponent(uri.replace(/^file:\/\//, ""));
    const root = context.worktree.replace(/[\\/]$/, "");
    return decoded.startsWith(`${root}/`) ? decoded.slice(root.length + 1) : null;
  }

  async function refreshQuickSymbols() {
    const tab = activeTab;
    const query = quickQuery.startsWith("@") ? quickQuery.slice(1).trim() : "";
    if (!tab || !workId || quickMode !== "symbol") return;
    quickSymbolQuery = query;
    try {
      const result = await getCodeWorkspaceSymbols({
        workId,
        language: tab.language,
        query,
      });
      if (quickSymbolQuery === query) quickSymbols = result;
    } catch {
      if (quickSymbolQuery === query) quickSymbols = [];
    }
  }

  async function chooseQuickSymbol(symbol = quickSymbolResults[quickIndex]) {
    const path = pathFromUri(symbol?.location?.uri);
    if (!symbol || !path) return;
    const line = (symbol.location?.range?.start?.line ?? 0) + 1;
    quickOpen = false;
    const tab = await codeWorkspace.open(workId, path, line);
    undertakings.setSelection({ path, line, entityId: null });
    await tick();
    if (tab) editor?.revealLine(line);
  }

  function chooseQuickLine() {
    const line = Number.parseInt(quickQuery.slice(1).trim(), 10);
    if (!Number.isFinite(line) || line < 1) return;
    quickOpen = false;
    editor?.revealLine(line);
    if (activeTab) {
      codeWorkspace.updateLine(activeTab.tabId, line);
      undertakings.setSelection({ path: activeTab.path, line, entityId: null });
    }
  }

  function chooseQuickResult() {
    if (quickMode === "symbol") void chooseQuickSymbol();
    else if (quickMode === "line") chooseQuickLine();
    else void chooseQuickFile();
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

  async function repairLanguageSupport() {
    if (!isCoLocatedWorkshop()) {
      openLanguagePackages();
      return;
    }
    repairingLanguage = true;
    surfaceError = null;
    try {
      const catalog = await fetchPackagesCatalog();
      if (!catalog) throw new Error("Package repair is unavailable here");
      const wanted = ["coding-engine", "langservers"];
      for (const packageId of wanted) {
        const row = catalog?.packages.find((entry) => entry.id === packageId);
        if (row && !row.installed) await installPackage(packageId);
      }
      lspRetry += 1;
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
      openLanguagePackages();
    } finally {
      repairingLanguage = false;
    }
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

  function reconcileOpenFiles() {
    const primary = activeTab;
    const secondary = secondaryTab;
    void reconcileExternal(primary);
    if (secondary?.tabId !== primary?.tabId) void reconcileExternal(secondary);
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
    if (tab?.tabId === activeTabId && editor) {
      const liveDraft = editor.getValue();
      if (liveDraft !== tab.draft) {
        codeWorkspace.updateDraft(tab.tabId, liveDraft);
        tab = { ...tab, draft: liveDraft };
      }
      editor.flushChanges();
    }
    const active = context;
    if (
      !tab ||
      !active?.leaseId ||
      active.leaseGeneration == null ||
      !codeWorkspace.isDirty(tab) ||
      saving
    ) return !tab || !codeWorkspace.isDirty(tab);
    saving = true;
    saveWhisper = "Saving…";
    if (saveWhisperTimer) clearTimeout(saveWhisperTimer);
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
      saveWhisper = "Saved";
      saveWhisperTimer = setTimeout(() => {
        saveWhisper = null;
      }, 1600);
      return true;
    } catch (err) {
      saveWhisper = null;
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
      captureEditorContext();
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
    if (contextPanel === "outline") await refreshSymbols();
  }

  async function refreshSymbols() {
    if (!activeTab || !documentUri || !lspClient) {
      symbols = [];
      return;
    }
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

  async function showProblems() {
    contextPanel = contextPanel === "problems" ? null : "problems";
    if (contextPanel !== "problems" || !activeTab || !lspClient) return;
    try {
      workspaceDiagnostics = await getCodeWorkspaceDiagnostics({
        workId,
        language: activeTab.language,
      });
      workspaceDiagnostics.sort((a, b) =>
        a.uri === documentUri ? -1 : b.uri === documentUri ? 1 : 0
      );
    } catch {
      workspaceDiagnostics = [];
    }
  }

  function activeLanguageEdits(result: unknown): Array<{
    newText?: string;
    range?: {
      start?: { line?: number; character?: number };
      end?: { line?: number; character?: number };
    };
  }> {
    if (Array.isArray(result)) {
      if (result.every((item) => item && typeof item === "object" && "range" in item)) {
        return result;
      }
      for (const action of result) {
        const edits = activeLanguageEdits(action);
        if (edits.length) return edits;
      }
      return [];
    }
    if (!result || typeof result !== "object" || !documentUri) return [];
    const value = result as {
      edit?: unknown;
      changes?: Record<string, unknown>;
      documentChanges?: Array<{ textDocument?: { uri?: string }; edits?: unknown }>;
    };
    if (value.edit) return activeLanguageEdits(value.edit);
    const direct = value.changes?.[documentUri];
    if (Array.isArray(direct)) return activeLanguageEdits(direct);
    const documentEdit = value.documentChanges?.find(
      (change) => change.textDocument?.uri === documentUri,
    );
    return Array.isArray(documentEdit?.edits)
      ? activeLanguageEdits(documentEdit.edits)
      : [];
  }

  function applyTextEdits(
    content: string,
    edits: Array<{ newText?: string; range?: { start?: { line?: number; character?: number }; end?: { line?: number; character?: number } } }>,
  ): string {
    const lines = content.split("\n");
    const offset = (position: { line?: number; character?: number } | undefined) => {
      const line = Math.max(0, Math.min(position?.line ?? 0, lines.length - 1));
      let total = 0;
      for (let index = 0; index < line; index += 1) total += lines[index].length + 1;
      return Math.min(content.length, total + Math.max(0, position?.character ?? 0));
    };
    let next = content;
    const changes = edits.map((edit) => ({
      from: offset(edit.range?.start),
      to: offset(edit.range?.end),
      text: edit.newText ?? "",
    })).sort((a, b) => b.from - a.from);
    for (const change of changes) {
      next = `${next.slice(0, change.from)}${change.text}${next.slice(change.to)}`;
    }
    return next;
  }

  async function applyWorkspaceEdit(result: unknown): Promise<boolean> {
    if (!result || typeof result !== "object" || !context?.leaseId || context.leaseGeneration == null) return false;
    const edit = (result as { edit?: unknown }).edit ?? result;
    const workspaceEdit = edit as {
      changes?: Record<string, unknown>;
      documentChanges?: Array<{ textDocument?: { uri?: string }; edits?: unknown }>;
    };
    const entries = workspaceEdit.changes
      ? Object.entries(workspaceEdit.changes)
      : (workspaceEdit.documentChanges ?? [])
          .filter((change) => change.textDocument?.uri && Array.isArray(change.edits))
          .map((change) => [change.textDocument!.uri!, change.edits] as const);
    if (entries.length === 0) return false;
    const files = [];
    for (const [uri, rawEdits] of entries) {
      const path = pathFromUri(uri);
      if (!path || !Array.isArray(rawEdits)) return false;
      const source = await getUndertakingSource(workId, path);
      files.push({
        path,
        expected_digest: source.digest,
        content: applyTextEdits(source.content, rawEdits),
      });
    }
    if (files.length === 0) return false;
    const saved = await saveUndertakingSources(workId, {
      files,
      lease_id: context.leaseId,
      generation: context.leaseGeneration,
    });
    for (const source of saved) {
      const tab = tabs.find((entry) => entry.path === source.path);
      if (tab) codeWorkspace.acceptSaved(tab.tabId, source);
    }
    return true;
  }

  async function runLanguageAction(
    action: "format" | "organize_imports" | "references" | "rename",
    newName?: string,
  ) {
    if (!activeTab || !documentUri || languageActionRunning) return;
    const cursor = editor?.getCursorPosition() ?? { line: 0, character: 0 };
    if (action === "rename" && !newName?.trim()) return;
    languageActionRunning = true;
    surfaceError = null;
    try {
      if (action === "rename" && !(await saveAll())) {
        throw new Error("Resolve unsaved files before renaming across the project");
      }
      const result = await requestCodeLanguageAction({
        workId,
        action,
        uri: documentUri,
        language: activeTab.language,
        line: cursor.line,
        character: cursor.character,
        newName: newName?.trim(),
      });
      if (action === "references") {
        references = Array.isArray(result) ? result : [];
        contextPanel = "references";
        return;
      }
      if (action === "rename" && await applyWorkspaceEdit(result)) return;
      const edits = activeLanguageEdits(result);
      if (edits.length) {
        editor?.applyLanguageEdits(edits);
      } else if (action === "rename") surfaceError = "The language server did not return an editable rename.";
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      languageActionRunning = false;
    }
  }

  function beginInlineRename() {
    if (!canRename || !editable) return;
    renameDraft = editor?.getSelectedWord() ?? "";
    renameOpen = true;
    void tick().then(() => {
      renameInput?.focus();
      renameInput?.select();
    });
  }

  async function commitInlineRename() {
    const name = renameDraft.trim();
    renameOpen = false;
    if (!name) return;
    await runLanguageAction("rename", name);
  }

  function cancelInlineRename() {
    renameOpen = false;
    renameDraft = "";
  }

  async function copyText(value: string) {
    try {
      await navigator.clipboard.writeText(value);
    } catch {
      surfaceError = "Could not copy to the clipboard.";
    }
  }

  function revealInExplorer(path: string) {
    undertakings.setSelection({ path, line: null, entityId: null });
  }

  function onBreadcrumbPath(path: string, isFile: boolean) {
    if (isFile) {
      undertakings.setSelection({ path, line: activeTab?.line ?? 1, entityId: null });
      return;
    }
    revealInExplorer(path);
  }

  function onEditorContextMenu(event: MouseEvent) {
    editorMenuX = event.clientX;
    editorMenuY = event.clientY;
    editorMenuOpen = true;
  }

  function onEditorMenuAction(action: CodeEditorMenuAction) {
    if (!activeTab) return;
    switch (action) {
      case "definition":
        editor?.goToDefinition();
        break;
      case "references":
        void runLanguageAction("references");
        break;
      case "rename":
        beginInlineRename();
        break;
      case "format":
        void runLanguageAction("format");
        break;
      case "organize_imports":
        void runLanguageAction("organize_imports");
        break;
      case "copy_path":
        void copyText(absolutePath);
        break;
      case "copy_relative_path":
        void copyText(activeTab.path);
        break;
      case "reveal":
        revealInExplorer(activeTab.path);
        break;
    }
  }

  async function reopenClosedTab() {
    if (!workId || !codeWorkspace.canReopenClosed(workId)) return;
    const tab = await codeWorkspace.reopenClosed(workId);
    if (!tab) return;
    undertakings.setSelection({ path: tab.path, line: tab.line, entityId: null });
    await tick();
    if (tab.line) editor?.revealLine(tab.line);
  }

  function symbolLine(symbol: CodeDocumentSymbol): number {
    return (
      symbol.selectionRange?.start?.line ?? symbol.range?.start?.line ?? 0
    ) + 1;
  }

  function containingSymbol(): string | null {
    return symbolTrail[symbolTrail.length - 1]?.name ?? null;
  }

  $effect(() => {
    if (!workId) return;
    setActiveCodeInsights(workId, {
      // Cursor/symbol context is captured only at an explicit handoff boundary.
      containing_symbol: null,
      diagnostics: problems.slice(0, 20).map((problem) =>
        `${activeTabPath || "current file"}:${problem.line} ${problem.message}`
      ),
      last_verification: taskResult
        ? `${taskResult.task.label}: ${taskResult.success ? "passed" : "failed"}${taskResult.exit_code != null ? ` (exit ${taskResult.exit_code})` : ""}`
        : null,
    });
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
    const cancelDeferred = deferCodeWorkspaceWork(() => {
      void getProjectTasks(id).then((tasks) => {
        if (cancelled) return;
        projectTasks = tasks;
        selectedTaskId = tasks.find((task) => task.kind === "verify")?.id ?? tasks[0]?.id ?? "";
      }).catch(() => {
        if (!cancelled) projectTasks = [];
      });
    });
    return () => { cancelled = true; cancelDeferred(); };
  });

  $effect(() => {
    const path = activeTabPath;
    if (!reviewAvailable || !workId || !path) {
      reviewChangedLines = [];
      return;
    }
    let cancelled = false;
    void getReviewFile(workId, path)
      .then((comparison) => {
        if (!cancelled) reviewChangedLines = comparison.changed_lines;
      })
      .catch(() => {
        if (!cancelled) reviewChangedLines = [];
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    void lspRetry;
    const root = context?.workId === workId ? context.worktree : null;
    if (!activeTabId || !root || !languageSupportsLsp(activeTabLanguage)) {
      lspClient = null;
      lspError = null;
      lspConnecting = false;
      return;
    }
    let cancelled = false;
    lspClient = null;
    lspError = null;
    lspConnecting = true;
    const cancelDeferred = deferCodeWorkspaceWork(() => {
      void getCodeWorkspaceLspClient({
        workId,
        workspaceRoot: root,
        language: activeTabLanguage,
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
    });
    return () => {
      cancelled = true;
      cancelDeferred();
    };
  });

  $effect(() => {
    void activeTabId;
    void documentUri;
    void lspClient;
    let cleanup = () => {};
    untrack(() => {
      const uri = documentUri;
      if (!activeTabId || !uri || !lspClient) {
        languageCapabilities = {};
        editorConventions = {};
        return;
      }
      const cancelDeferred = deferCodeWorkspaceWork(() => {
        void refreshSymbols();
        void getCodeLanguageCapabilities({ workId, uri, language: activeTabLanguage })
          .then((capabilities) => (languageCapabilities = capabilities))
          .catch(() => (languageCapabilities = {}));
        void getCodeEditorConventions({ workId, uri, language: activeTabLanguage })
          .then((conventions) => (editorConventions = conventions))
          .catch(() => (editorConventions = {}));
      });
      cleanup = cancelDeferred;
    });
    return cleanup;
  });

  $effect(() => {
    const tabId = activeTabId;
    if (!tabId) return;
    // The cleanup snapshots find state back into this map. Keep both the read
    // and write outside the effect dependency graph to avoid self-invalidation.
    const shouldOpen = untrack(() => findOpenByTabId[tabId] ?? false);
    void tick().then(() => {
      if (!editor) return;
      if (shouldOpen) editor.openFind();
      else codeEditorFind.hide(editor.getView());
    });
    return () => {
      // Persist the find state without making this effect depend on its own cleanup write.
      untrack(() => {
        findOpenByTabId = { ...findOpenByTabId, [tabId]: codeEditorFind.open };
      });
    };
  });

  $effect(() => {
    const tabId = activeTabId;
    const initialLine = untrack(() => activeTabLine);
    editorSelection = null;
    cursorLine = initialLine ?? 1;
    cursorColumn = 1;
    if (initialLine) {
      void tick().then(() => {
        if (activeTabId === tabId) editor?.revealLine(initialLine);
      });
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

  onDestroy(() => {
    if (linePersistTimer) clearTimeout(linePersistTimer);
  });
</script>

<section class="flex flex-col overflow-hidden rounded-lg border border-surface-500/35 bg-surface-950/45 {fill ? 'min-h-0 flex-1' : 'min-h-[26rem]'}">
  {#if tabs.length > 0}
    <CodeDocumentTabStrip
      {tabs}
      activeTabId={activeTab?.tabId ?? null}
      {tabLabel}
      onActivate={activate}
      onClose={close}
      onOpenToSide={(tab) => codeWorkspace.openToSide(tab.tabId)}
      onCopyPath={(tab) => void copyText(
        context?.worktree
          ? `${context.worktree.replace(/[\\/]$/, "")}/${tab.path}`
          : tab.path,
      )}
    />
  {/if}

  {#if activeTab}
    <header class="flex shrink-0 items-center justify-between gap-1.5 border-b border-surface-500/30 px-2 py-1 sm:gap-3 sm:px-2.5">
      <div class="flex min-w-0 flex-1 items-center gap-1.5">
        <div class="flex shrink-0 items-center">
          <button type="button" class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-25" aria-label="Go back" title="Go back" disabled={!codeWorkspace.canNavigate(workId, -1)} onclick={() => void navigate(-1)}><ArrowLeft size={11} /></button>
          <button type="button" class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-25" aria-label="Go forward" title="Go forward" disabled={!codeWorkspace.canNavigate(workId, 1)} onclick={() => void navigate(1)}><ArrowRight size={11} /></button>
        </div>
        <CodeBreadcrumbs
          path={activeTab.path}
          symbols={symbolTrail}
          onPathSegment={onBreadcrumbPath}
          onSymbol={(line) => editor?.revealLine(line)}
        />
        {#if saveWhisper}
          <span class="shrink-0 text-[9px] text-primary-200/90">{saveWhisper}</span>
        {:else if dirty}
          <span class="shrink-0 text-[9px] text-primary-300/80">unsaved</span>
        {/if}
        {#if lspConnecting}
          <span class="shrink-0 text-[9px] text-surface-500">understanding…</span>
        {:else if lspError}
          <span class="shrink-0 text-[9px] text-surface-500">editing only</span>
        {/if}
      </div>
      <div class="flex shrink-0 items-center gap-1">
        <button
          type="button"
          class="rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100"
          title={`Find (${formatShortcut("F")})`}
          onclick={() => editor?.openFind()}
        >Find</button>
        <details class="relative">
          <summary class="cursor-pointer list-none rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100 [&::-webkit-details-marker]:hidden" title="Editor options">View</summary>
          <div class="absolute right-0 top-full z-20 mt-1 w-40 rounded-md border border-surface-500/40 bg-surface-900 p-1 shadow-xl">
            <button type="button" class="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-[10px] text-surface-300 hover:bg-surface-800" onclick={toggleWordWrap}>
              <span>Word wrap</span>
              <span class="text-surface-500">{wordWrap ? "On" : "Off"}</span>
            </button>
            <button type="button" class="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-[10px] text-surface-300 hover:bg-surface-800" onclick={toggleLineNumbers}>
              <span>Line numbers</span>
              <span class="text-surface-500">{showLineNumbers ? "On" : "Off"}</span>
            </button>
          </div>
        </details>
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

    <div class="flex shrink-0 items-center gap-2 overflow-x-auto border-b border-surface-500/20 bg-surface-950/50 px-2.5 py-0.5 text-[9px] text-surface-500" aria-label="Operator status">
      <span class="font-medium text-surface-300">{operatorLabel}</span>
      {#if dirtyCount > 0}
        <span aria-hidden="true">·</span>
        <span class="text-primary-300/90">{dirtyCount} dirty</span>
      {/if}
      <span aria-hidden="true">·</span>
      <button type="button" class="hover:text-surface-200" onclick={() => void showProblems()}>
        {Math.max(problems.length, workspaceProblemRows.length)} issues
      </button>
      {#if lastVerifyLabel}
        <span aria-hidden="true">·</span>
        <button
          type="button"
          class="{taskResult?.success ? 'text-emerald-300/90' : 'text-rose-300/90'} hover:underline"
          title="Jump to last verification"
          onclick={() => {
            const location = taskResult?.locations[0];
            if (location) void openTaskLocation(location.path, location.line);
          }}
        >{lastVerifyLabel}</button>
      {/if}
    </div>

    {#if surfaceError || activeTab.error || codeWorkspace.workspaceErrorByWorkId[workId]}
      <p class="shrink-0 border-b border-amber-500/30 bg-amber-950/25 px-2.5 py-1.5 text-[10px] text-amber-100">
        {humanizeForgeMessage(surfaceError || activeTab.error || codeWorkspace.workspaceErrorByWorkId[workId] || "")}
      </p>
    {/if}
    {#if externalVersions[activeTab.tabId]}
      <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-amber-500/30 bg-amber-950/20 px-2.5 py-1.5 text-[10px] text-amber-100">
        <span class="min-w-40 flex-1">This file changed elsewhere. Your draft is safe.</span>
        <button type="button" class="rounded px-1.5 py-0.5 hover:bg-white/10" onclick={() => (comparingTabId = activeTab.tabId)}>Compare</button>
        <button type="button" class="rounded px-1.5 py-0.5 hover:bg-white/10" onclick={() => useProjectVersion(activeTab)}>Use project version</button>
        <button type="button" class="rounded bg-amber-500/20 px-1.5 py-0.5" onclick={() => keepDraft(activeTab)}>Keep my draft</button>
      </div>
    {/if}

    <div class="min-h-0 flex-1 {secondaryTab ? 'grid grid-cols-1 overflow-y-auto md:grid-cols-2 md:overflow-hidden' : 'flex overflow-hidden'}">
      <div class="relative min-h-0 min-w-0 flex-1" onfocusin={() => (focusedSide = false)}>
        {#if editorSelection?.text && onHandoffToAgent && !agentHasControl}
          <div class="absolute right-3 top-2 z-20 flex max-w-[calc(100%-1.5rem)] items-center gap-1 overflow-x-auto rounded-md border border-primary-500/30 bg-surface-950/95 px-1.5 py-1 shadow-xl" aria-label="Selected code actions">
            <span class="mr-1 flex shrink-0 items-center gap-1 text-[9px] text-primary-200/80"><Sparkles size={10} />Selection</span>
            <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Help me understand the selected code and answer my questions about it.")}>Ask</button>
            <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Change the selected code. Ask only if the intended change is ambiguous.")}>Change</button>
            {#if problems.length > 0}<button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Fix the relevant issue in the selected code and verify the result.")}>Fix</button>{/if}
            <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Explain the selected code clearly, including its role and important behavior.")}>Explain</button>
            <button type="button" class="code-intent-action" disabled={saving} onclick={() => void handoffToAgent("Add the most valuable focused test for the selected code and run the relevant check.")}>Add test</button>
            {#if canReference}<button type="button" class="code-intent-action" disabled={languageActionRunning} onclick={() => void runLanguageAction("references")}>Find uses</button>{/if}
            {#if canRename && editable}<button type="button" class="code-intent-action" disabled={languageActionRunning} onclick={() => beginInlineRename()}>Rename</button>{/if}
          </div>
        {/if}
        {#if activeTab.loading}
          <div class="absolute inset-0 z-10 flex items-center justify-center bg-surface-950/70 text-xs text-surface-400">
            <LoaderCircle size={14} class="mr-2 animate-spin" />Opening source…
          </div>
        {/if}
        {#if !activeTab.loading && activeTab.digest}
          {@const editorTab = activeTab}
          {#key editorTab.tabId}
            <CodeMirrorHost
              bind:this={editor}
              value={editorTab.draft}
              languageId={editorTab.language}
              {documentUri}
              lspLanguageId={editorTab.language}
              client={lspClient}
              readOnly={!editable}
              contentSyncKey={editorTab.syncKey}
              changedLines={reviewChangedLines}
              conventionIndentStyle={editorConventions.indent_style ?? null}
              conventionTabSize={Number.parseInt(editorConventions.indent_size ?? editorConventions.tab_width ?? "", 10) || null}
              {wordWrap}
              {showLineNumbers}
              onchange={(value) => codeWorkspace.updateDraft(editorTab.tabId, value)}
              onCursorChanged={(cursor) => handleCursorChanged(editorTab, cursor)}
              onSelectionChanged={(selection) => (editorSelection = selection.text ? selection : null)}
              onProblemsChanged={syncProblems}
              onContextMenu={onEditorContextMenu}
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
            {contextPanel === "problems" ? "Issues" : contextPanel === "references" ? "Uses" : "Structure"}
          </span>
          <button type="button" class="rounded p-0.5 text-surface-500 hover:text-surface-200" aria-label="Close context panel" onclick={() => (contextPanel = null)}><X size={11} /></button>
        </div>
        {#if contextPanel === "problems"}
          {#if problems.length === 0 && workspaceProblemRows.length === 0}
            <p class="px-3 py-3 text-[10px] text-surface-500">No issues found in this project.</p>
          {:else}
            {#each workspaceProblemRows as problem, index (`${problem.path}:${problem.line}:${problem.message}:${index}`)}
              <button
                type="button"
                class="flex w-full items-start gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
                onclick={() => {
                  const file = quickFiles.find((entry) => entry.path === problem.path);
                  if (file) void chooseQuickFile(file).then(() => editor?.revealLine(problem.line));
                  else if (problem.path === activeTab?.path) editor?.revealLine(problem.line);
                }}
              >
                <CircleAlert size={11} class={problem.severity === 1 ? "mt-0.5 shrink-0 text-rose-300" : "mt-0.5 shrink-0 text-amber-300"} />
                <span class="min-w-0 flex-1 text-[10px] text-surface-300"><span class="text-surface-500">{problem.path} · </span>{problem.message}</span>
                <span class="shrink-0 font-mono text-[9px] text-surface-500">{problem.line}</span>
              </button>
            {/each}
          {/if}
        {:else if contextPanel === "references"}
          {#if references.length === 0}
            <p class="px-3 py-3 text-[10px] text-surface-500">No other uses found.</p>
          {:else}
            {#each references as reference, index (`${reference.uri}:${reference.range?.start?.line}:${index}`)}
              {@const referencePath = pathFromUri(reference.uri)}
              {@const referenceLine = (reference.range?.start?.line ?? 0) + 1}
              <button
                type="button"
                class="flex w-full items-center gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
                onclick={async () => {
                  if (!referencePath) return;
                  contextPanel = null;
                  await codeWorkspace.open(workId, referencePath, referenceLine);
                  undertakings.setSelection({ path: referencePath, line: referenceLine, entityId: null });
                  await tick();
                  editor?.revealLine(referenceLine);
                }}
              >
                <FileCode2 size={11} class="shrink-0 text-primary-300/70" />
                <span class="min-w-0 flex-1 truncate text-[10px] text-surface-300">{referencePath ?? reference.uri}</span>
                <span class="font-mono text-[9px] text-surface-500">{referenceLine}</span>
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
          class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200"
          class:bg-surface-800={contextPanel === "problems"}
          onclick={() => void showProblems()}
        ><CircleAlert size={10} />{Math.max(problems.length, workspaceProblemRows.length)}</button>
        <button
          type="button"
          class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-35"
          class:bg-surface-800={contextPanel === "outline"}
          disabled={!lspClient}
          onclick={() => void showOutline()}
        ><ListTree size={10} />Structure</button>
        {#if lspError}
          <button type="button" class="rounded px-1.5 py-0.5 text-[9px] text-amber-300/80 hover:bg-surface-800 hover:text-amber-200 disabled:opacity-40" disabled={repairingLanguage} title={lspError} onclick={() => void repairLanguageSupport()}>{repairingLanguage ? "Repairing…" : isCoLocatedWorkshop() ? "Repair language support" : "Add language support"}</button>
        {/if}
        {#if canFormat && editable}
          <button type="button" class="rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-35" disabled={languageActionRunning} onclick={() => void runLanguageAction("format")}>Format</button>
        {/if}
        {#if canCodeAction && editable}
          <button type="button" class="rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-35" disabled={languageActionRunning} onclick={() => void runLanguageAction("organize_imports")}>Organize imports</button>
        {/if}
        <button
          type="button"
          class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-35"
          class:bg-surface-800={terminalDockOpen}
          disabled={!terminalAvailable && !selectedTask}
          onclick={() => {
            if (runningTask) void stopDetectedTask();
            else if (selectedTask) void runDetectedTask();
            else void toggleTerminalDock();
          }}
          title={selectedTask ? selectedTask.argv.join(" ") : "Toggle Terminal (Ctrl+`)"}
        >{#if runningTask}<X size={10} />Stop {selectedTask?.label}{:else}<SquareTerminal size={10} />{selectedTask?.label ?? "Terminal"}{/if}</button>
        {#if projectTasks.some((task) => task.kind === "test")}
          <button type="button" class="rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200" class:bg-surface-800={testsOpen} onclick={() => void toggleTests()}>Tests</button>
        {/if}
        {#if projectTasks.length > 1}
          <select class="max-w-24 rounded bg-transparent py-0.5 text-[9px] text-surface-500 outline-none" aria-label="Project command" bind:value={selectedTaskId}>
            {#each projectTasks as task (task.id)}
              <option value={task.id}>{task.label}</option>
            {/each}
          </select>
        {/if}
        {#if terminalAvailable}
          <button type="button" class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[9px] text-surface-500 hover:bg-surface-800 hover:text-surface-200" class:bg-surface-800={terminalDockOpen} onclick={() => void toggleTerminalDock()} title="Toggle Terminal (Ctrl+`)"><SquareTerminal size={10} />Shell</button>
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
      <div class="flex shrink-0 items-center gap-1.5 text-[9px] text-surface-500">
        <span class="hidden items-center gap-1 sm:inline-flex">
          <kbd class="vault-kbd">{formatShortcut("F")}</kbd> find
          <span class="text-surface-600" aria-hidden="true">·</span>
          <kbd class="vault-kbd">{formatShortcut("S")}</kbd> save
          <span class="text-surface-600" aria-hidden="true">·</span>
          <kbd class="vault-kbd">{formatShortcut("P")}</kbd> open
          <span class="text-surface-600" aria-hidden="true">·</span>
          <kbd class="vault-kbd">⌃`</kbd> terminal
        </span>
        <span class="font-mono tabular-nums">Ln {cursorLine}, Col {cursorColumn}</span>
        <span class="text-surface-600" aria-hidden="true">·</span>
        <span>{indentStatusLabel}</span>
        <span class="text-surface-600" aria-hidden="true">·</span>
        <span class="font-mono">{activeTab.language}</span>
        <span class="text-surface-600" aria-hidden="true">·</span>
        <span>
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
      </div>
    </footer>
    {#if taskResult}
      <div class="shrink-0 border-t {taskResult.success ? 'border-emerald-500/25 bg-emerald-950/20 text-emerald-200' : 'border-rose-500/30 bg-rose-950/25 text-rose-200'}">
      <button type="button" class="flex w-full items-center justify-between gap-2 px-2.5 py-1 text-left text-[9px]" title="Run this check again" onclick={() => void runDetectedTask()}>
        <span>{taskResult.success ? "Passed" : "Needs attention"} · {taskResult.task.label}</span>
        <span class="text-current opacity-60">Rerun · {(taskResult.duration_ms / 1000).toFixed(1)}s{taskResult.exit_code != null ? ` · exit ${taskResult.exit_code}` : ""}</span>
      </button>
      {#each taskResult.locations.slice(0, 5) as location (`${location.path}:${location.line}:${location.column}`)}
        <button type="button" class="flex w-full items-center gap-2 border-t border-current/10 px-2.5 py-1 text-left text-[9px] hover:bg-white/5" onclick={() => void openTaskLocation(location.path, location.line)}>
          <span class="min-w-0 flex-1 truncate">{location.message || location.path}</span>
          <span class="shrink-0 font-mono opacity-60">{location.path}:{location.line}</span>
        </button>
      {/each}
      </div>
    {/if}
    {#if testsOpen}
      <div class="max-h-44 shrink-0 overflow-y-auto border-t border-surface-500/25 bg-surface-950/90">
        <div class="sticky top-0 flex items-center justify-between bg-surface-950 px-2.5 py-1 text-[9px] uppercase tracking-wider text-surface-500"><span>Project tests</span><span>{projectTests.length}</span></div>
        {#if projectTests.length === 0}
          <p class="px-3 py-3 text-[10px] text-surface-500">No individual tests were discovered. The project test command still works.</p>
        {:else}
          {#each projectTests as test (test.id)}
            <div class="flex items-center border-t border-surface-500/15">
              <button type="button" class="min-w-0 flex-1 truncate px-3 py-1.5 text-left text-[10px] text-surface-300 hover:bg-surface-800/60" onclick={() => void openTaskLocation(test.path, test.line)}>{test.label}<span class="ml-2 font-mono text-[9px] text-surface-600">{test.path}:{test.line}</span></button>
              <button type="button" class="mr-2 rounded px-1.5 py-0.5 text-[9px] text-primary-300 hover:bg-surface-800 disabled:opacity-40" disabled={runningTask} onclick={() => void runDetectedTask(test)}>Run</button>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
  {:else}
    <div class="flex min-h-0 flex-1 flex-col">
      <div class="flex min-h-72 flex-1 items-center justify-center p-8 text-center">
        <div class="max-w-sm">
          {#if needsProvision}
            <FileCode2 size={24} class="mx-auto text-surface-600" />
            <p class="mt-2 text-xs font-medium text-surface-300">Set up this project</p>
            <p class="mt-1 text-[10px] leading-relaxed text-surface-500">
              {landError || "Create the working copy so the tree and editor can open."}
            </p>
            {#if onProvision}
              <button
                type="button"
                class="mt-3 rounded bg-primary-500/80 px-3 py-1.5 text-[11px] font-medium text-surface-50"
                onclick={() => void onProvision()}
              >Set up project</button>
            {/if}
          {:else if landError}
            <FileCode2 size={24} class="mx-auto text-amber-500/70" />
            <p class="mt-2 text-xs font-medium text-amber-100">Could not open the working set</p>
            <p class="mt-1 text-[10px] leading-relaxed text-amber-100/80">{humanizeForgeMessage(landError)}</p>
            <div class="mt-3 flex flex-wrap items-center justify-center gap-2">
              <button
                type="button"
                class="rounded bg-primary-500/80 px-3 py-1.5 text-[11px] font-medium text-surface-50"
                onclick={() => void showQuickOpen()}
              >Open file <kbd class="vault-kbd ml-1">{formatShortcut("P")}</kbd></button>
              {#if terminalAvailable}
                <button
                  type="button"
                  class="rounded border border-surface-500/40 px-3 py-1.5 text-[11px] text-surface-200 hover:bg-surface-800"
                  disabled={dockBusy}
                  onclick={() => void toggleTerminalDock(true)}
                >Terminal <kbd class="vault-kbd ml-1">⌃`</kbd></button>
              {/if}
            </div>
          {:else}
            <FileCode2 size={24} class="mx-auto text-surface-600" />
            <p class="mt-2 text-xs font-medium text-surface-300">Open a file</p>
            <p class="mt-1 text-[10px] leading-relaxed text-surface-500">
              Jump in with Quick Open, or pick a path from the project tree.
            </p>
            <div class="mt-3 flex flex-wrap items-center justify-center gap-2">
              <button
                type="button"
                class="rounded bg-primary-500/80 px-3 py-1.5 text-[11px] font-medium text-surface-50"
                onclick={() => void showQuickOpen()}
              >Open file <kbd class="vault-kbd ml-1">{formatShortcut("P")}</kbd></button>
              {#if terminalAvailable}
                <button
                  type="button"
                  class="rounded border border-surface-500/40 px-3 py-1.5 text-[11px] text-surface-200 hover:bg-surface-800"
                  disabled={dockBusy}
                  onclick={() => void toggleTerminalDock(true)}
                >Terminal <kbd class="vault-kbd ml-1">⌃`</kbd></button>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/if}
  <CodeTerminalDock
    open={terminalDockOpen}
    sessionId={dockSessionId}
    {workId}
    title="Terminal"
    onClose={() => (terminalDockOpen = false)}
    onPopOut={() => void popOutTerminal()}
  />
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
        <input bind:this={quickInput} class="min-w-0 flex-1 bg-transparent py-2.5 text-sm text-surface-100 outline-none" placeholder="File, @symbol, or :line" bind:value={quickQuery} oninput={() => { quickIndex = 0; void refreshQuickSymbols(); }} onkeydown={(event) => {
          if (event.key === "ArrowDown") { event.preventDefault(); quickIndex = Math.min(quickIndex + 1, quickResultCount - 1); }
          if (event.key === "ArrowUp") { event.preventDefault(); quickIndex = Math.max(quickIndex - 1, 0); }
          if (event.key === "Enter") { event.preventDefault(); chooseQuickResult(); }
        }} />
        <span class="text-[9px] text-surface-600">⌘P</span>
      </div>
      <div class="max-h-[50vh] overflow-y-auto py-1">
        {#if quickMode === "line"}
          <button type="button" class="flex w-full items-center gap-2 px-3 py-2 text-left text-surface-300 hover:bg-surface-800" onclick={chooseQuickLine}>
            <span class="font-mono text-xs text-primary-300">:{quickQuery.slice(1).trim() || "line"}</span>
            <span class="text-[10px] text-surface-500">Go to a line in {activeTab?.title}</span>
          </button>
        {:else if quickMode === "symbol" && quickSymbolResults.length === 0}
          <p class="px-3 py-3 text-xs text-surface-500">No matching project symbols.</p>
        {:else if quickMode === "symbol"}
          {#each quickSymbolResults as symbol, index (`${symbol.name}:${symbol.location?.uri}:${symbol.location?.range?.start?.line}`)}
            <button type="button" class="flex w-full items-center gap-2 px-3 py-1.5 text-left {index === quickIndex ? 'bg-surface-800 text-surface-100' : 'text-surface-400 hover:bg-surface-900'}" onmouseenter={() => (quickIndex = index)} onclick={() => void chooseQuickSymbol(symbol)}>
              <ListTree size={12} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-xs">{symbol.name}</span>
              <span class="min-w-0 max-w-[60%] truncate font-mono text-[9px] text-surface-600">{symbol.containerName ?? pathFromUri(symbol.location?.uri) ?? ""}</span>
            </button>
          {/each}
        {:else if quickLoading}
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

<CodeEditorContextMenu
  open={editorMenuOpen}
  x={editorMenuX}
  y={editorMenuY}
  canDefinition={canDefinition}
  canReference={canReference}
  canRename={canRename}
  canFormat={canFormat}
  canOrganize={canCodeAction}
  {editable}
  onAction={onEditorMenuAction}
  onClose={() => (editorMenuOpen = false)}
/>

{#if renameOpen}
  <div class="fixed inset-0 z-[130] flex items-start justify-center px-4 pt-[18vh]">
    <button type="button" class="absolute inset-0 bg-black/35" aria-label="Cancel rename" onclick={cancelInlineRename}></button>
    <div class="relative w-full max-w-sm overflow-hidden rounded-lg border border-surface-500/50 bg-surface-950 shadow-2xl" role="dialog" aria-modal="true" aria-label="Rename symbol">
      <div class="border-b border-surface-500/30 px-3 py-2">
        <p class="text-xs font-medium text-surface-100">Rename symbol</p>
        <p class="text-[10px] text-surface-500">Applies across the project when the language server supports it.</p>
      </div>
      <div class="px-3 py-2.5">
        <input
          bind:this={renameInput}
          class="w-full rounded border border-surface-500/40 bg-surface-900 px-2.5 py-1.5 text-sm text-surface-100 outline-none focus:border-primary-400/50"
          bind:value={renameDraft}
          spellcheck="false"
          aria-label="New symbol name"
          onkeydown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void commitInlineRename();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              cancelInlineRename();
            }
          }}
        />
      </div>
      <footer class="flex justify-end gap-2 border-t border-surface-500/30 px-3 py-2">
        <button type="button" class="rounded px-2 py-1 text-[10px] text-surface-400 hover:bg-surface-800" onclick={cancelInlineRename}>Cancel</button>
        <button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-[10px] font-medium text-white disabled:opacity-40" disabled={!renameDraft.trim() || languageActionRunning} onclick={() => void commitInlineRename()}>Rename</button>
      </footer>
    </div>
  </div>
{/if}

<svelte:window
  onfocus={reconcileOpenFiles}
  onkeydown={(event) => {
    onWindowKeydown(event);
    if (event.defaultPrevented) return;
    if (renameOpen || quickOpen || editorMenuOpen) return;
    const command = event.metaKey || event.ctrlKey;
    if (command && event.shiftKey && event.key.toLowerCase() === "t") {
      event.preventDefault();
      void reopenClosedTab();
      return;
    }
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
    if (command && event.key === "`") {
      event.preventDefault();
      void toggleTerminalDock();
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
    if (event.key === "F2" && canRename && editable && activeTab) {
      event.preventDefault();
      beginInlineRename();
      return;
    }
    if (event.key === "Escape" && contextPanel) contextPanel = null;
  }}
/>
