<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import {
    CircleAlert,
    ArrowLeft,
    ArrowRight,
    FileCode2,
    ListTree,
    LoaderCircle,
    Pencil,
    RotateCcw,
    Save,
    SquareTerminal,
    GitPullRequestArrow,
    Orbit,
    Play,
    Sparkles,
    UserRound,
    X,
    Search,
  } from "@lucide/svelte";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import CodeBreadcrumbs from "$lib/components/code/CodeBreadcrumbs.svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { buildTextDiff } from "$lib/diff/buildTextDiff";
  import CodeEditorContextMenu, {
    type CodeEditorMenuAction,
  } from "$lib/components/code/CodeEditorContextMenu.svelte";
  import CodeTerminalDock from "$lib/components/work/CodeTerminalDock.svelte";
  import { openTrackedTerminal } from "$lib/utils/undertakingWorkspace";
  import type { LSPClient } from "@codemirror/lsp-client";
  import {
    acquireCodeWorkspaceLspClient,
    getCodeEditorConventions,
    type CodeDocumentSymbol,
    type CodeWorkspaceSymbol,
  } from "$lib/code/codingEngineClient";
  import {
    pathToFileUri,
    workspaceRelativePathFromUri,
  } from "$lib/code/codeDocumentUri";
  import { codeEditorViewRegistry } from "$lib/code/codeEditorViewRegistry";
  import type { CodeLanguageNavigationKind } from "$lib/code/codeLanguageNavigation";
  import { containingSymbolTrail } from "$lib/code/codeDocumentSymbols";
  import {
    languageSupportsLsp,
    resolveCodeEditorLanguage,
  } from "$lib/code/codeEditorLanguageRegistry";
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
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { setActiveCodeInsights } from "$lib/utils/undertakingWorkspace";
  import { deferCodeWorkspaceWork } from "$lib/utils/codeWorkspaceTrace";
  import { fetchPackagesCatalog, installPackage } from "$lib/utils/packagesApi";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { titleWithShortcut } from "$lib/utils/keyboardShortcutsCatalog";
  import {
    readCodeEditorFontSize,
    readCodeEditorLineNumbers,
    readCodeEditorTabSize,
    readCodeEditorWordWrap,
    writeCodeEditorFontSize,
    writeCodeEditorLineNumbers,
    writeCodeEditorTabSize,
    writeCodeEditorWordWrap,
    type CodeEditorFontSize,
  } from "$lib/config/codeEditorPreferences";
  import { codeEditorFind } from "$lib/stores/codeEditorFind.svelte";
  import { codeEditorStatus } from "$lib/stores/codeEditorStatus.svelte";

  interface Props {
    fill?: boolean;
    workId?: string;
    resourcePath?: string | null;
    interactive?: boolean;
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
    workId: boundWorkId = "",
    resourcePath = null,
    interactive = true,
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
  let fontSize = $state<CodeEditorFontSize>(readCodeEditorFontSize());
  let tabSizePref = $state(readCodeEditorTabSize());
  let editorPrefsEpoch = $state(0);
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
  let languageCapabilities = $state<Record<string, unknown>>({});
  let editorConventions = $state<{ indent_style?: "space" | "tab"; indent_size?: string; tab_width?: string }>({});
  let references = $state<Array<{ uri?: string; range?: { start?: { line?: number } } }>>([]);
  let languageActionRunning = $state(false);
  let repairingLanguage = $state(false);
  let lspRetry = $state(0);
  let cursorLine = $state(1);
  let cursorTotalLines = $state(1);
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
  const statusOwnerId = `code-editor-${Math.random().toString(36).slice(2)}`;

  const context = $derived(undertakings.active);
  const detail = $derived(undertakings.detail);
  const workId = $derived(boundWorkId || detail?.id || context?.workId || "");
  const tabs = $derived(workId ? codeWorkspace.orderedTabsFor(workId) : []);
  const activeTab = $derived.by(() => {
    if (resourcePath) {
      return (
        codeWorkspace.tabs.find(
          (tab) => tab.work_id === workId && tab.path === resourcePath,
        ) ?? null
      );
    }
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
  const editable = $derived(
    Boolean(
      interactive &&
      context?.workId === workId &&
      context.leaseId &&
      context.leaseGeneration != null,
    ),
  );
  const agentHasControl = $derived(
    Boolean(context?.executorKind && context.executorKind !== "human"),
  );
  const canBeginEdit = $derived(
    Boolean(
      interactive &&
        !agentHasControl &&
        detail?.allowed_actions.begin_attempt.allowed,
    ),
  );
  /** Soft lease: typing is allowed when a human attempt can begin. */
  const bufferInteractive = $derived(editable || canBeginEdit);
  let beginEditPromise = $state<Promise<void> | null>(null);
  const statusControlLabel = $derived.by(() => {
    if (agentHasControl) {
      return context?.executorKind === "cursor" ? "Cursor working" : "Codex working";
    }
    if (editable && context?.boundTerminalSessionIds.length) return "You + Terminal";
    if (editable) return "You editing";
    return "Ready";
  });
  const landError = $derived(workId ? codeWorkspace.workspaceErrorByWorkId[workId] ?? null : null);
  const needsProvision = $derived(
    Boolean(detail && !detail.environment && detail.allowed_actions.provision.allowed),
  );
  // A document identity is only meaningful inside the worktree that owns its
  // work id. Detail and global active context can briefly diverge while shell
  // tabs activate or undertaking detail refreshes; never combine the two.
  const workspaceRoot = $derived.by(() => {
    if (!workId) return null;
    if (detail?.id === workId && detail.environment?.worktree) {
      return detail.environment.worktree;
    }
    return context?.workId === workId ? context.worktree : null;
  });
  const documentUri = $derived.by(() => {
    if (!activeTabPath || !workspaceRoot) return null;
    return pathToFileUri(
      `${workspaceRoot.replace(/[\\/]$/, "")}/${activeTabPath}`,
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
  const workspaceProblemRows = $derived(
    [...problems]
      .map((problem) => ({
        path: activeTab?.path ?? "Current file",
        line: problem.line,
        message: problem.message,
        severity: problem.severity === "error" ? 1 : 2,
      }))
      .sort((a, b) => a.severity - b.severity || a.line - b.line),
  );
  const canReference = $derived(Boolean(languageCapabilities.referencesProvider));
  const canRename = $derived(Boolean(languageCapabilities.renameProvider));
  const canFormat = $derived(Boolean(languageCapabilities.documentFormattingProvider));
  const canCodeAction = $derived(Boolean(languageCapabilities.codeActionProvider));
  const canDefinition = $derived(Boolean(languageCapabilities.definitionProvider ?? lspClient));
  const canDeclaration = $derived(Boolean(languageCapabilities.declarationProvider));
  const canTypeDefinition = $derived(Boolean(languageCapabilities.typeDefinitionProvider));
  const canImplementation = $derived(Boolean(languageCapabilities.implementationProvider));
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
    await lmeWorkspace.openCodeFile(workId, path, { line });
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

  function cycleFontSize() {
    const order: CodeEditorFontSize[] = [12, 13, 14, 15, 16];
    const next = order[(order.indexOf(fontSize) + 1) % order.length] ?? 13;
    fontSize = next;
    writeCodeEditorFontSize(next);
    editorPrefsEpoch += 1;
  }

  function cycleTabSize() {
    const order = [2, 4, 8] as const;
    const next = order[(order.indexOf(tabSizePref as 2 | 4 | 8) + 1) % order.length] ?? 2;
    tabSizePref = next;
    writeCodeEditorTabSize(next);
    editorPrefsEpoch += 1;
  }

  function handleCursorChanged(
    tab: CodeDocumentTab,
    cursor: { line: number; totalLines: number; column: number },
  ) {
    cursorLine = cursor.line;
    cursorTotalLines = cursor.totalLines;
    cursorColumn = cursor.column;
    if (linePersistTimer) clearTimeout(linePersistTimer);
    linePersistTimer = setTimeout(() => {
      linePersistTimer = null;
      codeWorkspace.updateLine(tab.tabId, cursor.line);
      lmeWorkspace.updateCodeLocation(tab.work_id, tab.path, cursor.line);
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
    const tab = await lmeWorkspace.openCodeFile(workId, file.path, { line: 1 });
    undertakings.setSelection({ path: file.path, line: 1, entityId: null });
    await tick();
    if (tab) editor?.focusEditor();
  }

  function pathFromUri(
    uri = "",
    root = workspaceRoot ?? context?.worktree ?? "",
  ): string | null {
    return uri && root ? workspaceRelativePathFromUri(uri, root) : null;
  }

  function languageForWorkspacePath(path: string): string {
    const extension = path.split(".").pop()?.toLowerCase() ?? "";
    const resolved = resolveCodeEditorLanguage(extension);
    return resolved === "plaintext" ? activeTabLanguage : resolved;
  }

  async function refreshQuickSymbols() {
    const tab = activeTab;
    const query = quickQuery.startsWith("@") ? quickQuery.slice(1).trim() : "";
    const client = lspClient;
    if (!tab || !workId || quickMode !== "symbol" || !client) return;
    quickSymbolQuery = query;
    try {
      client.sync();
      const result = await client.request<
        { query: string },
        CodeWorkspaceSymbol[] | null
      >("workspace/symbol", { query });
      if (quickSymbolQuery === query) quickSymbols = Array.isArray(result) ? result : [];
    } catch {
      if (quickSymbolQuery === query) quickSymbols = [];
    }
  }

  async function chooseQuickSymbol(symbol = quickSymbolResults[quickIndex]) {
    const path = pathFromUri(symbol?.location?.uri);
    if (!symbol || !path) return;
    const line = (symbol.location?.range?.start?.line ?? 0) + 1;
    quickOpen = false;
    const tab = await lmeWorkspace.openCodeFile(workId, path, { line });
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
      lmeWorkspace.updateCodeLocation(activeTab.work_id, activeTab.path, line);
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
    await lmeWorkspace.openCodeFile(workId, tab.path, {
      line: tab.line,
      recordNavigation: false,
    });
    undertakings.setSelection({ path: tab.path, line: tab.line, entityId: null });
    await tick();
    if (tab.line) editor?.revealLine(tab.line);
  }

  async function navigateLanguageLocation(kind: CodeLanguageNavigationKind) {
    const sourceTab = activeTab;
    const sourceEditor = editor;
    const root = workspaceRoot;
    if (!sourceTab || !sourceEditor || !root) return;
    const sourceCursor = sourceEditor.getCursorPosition();
    surfaceError = null;
    try {
      const target = await sourceEditor.goToLanguageLocation(kind);
      if (!target) return;
      const targetPath = pathFromUri(target.uri, root);
      if (!targetPath) {
        throw new Error("The language server returned a location outside this project");
      }
      codeWorkspace.recordNavigationLocation(
        workId,
        sourceTab.path,
        sourceCursor.line + 1,
      );
      codeWorkspace.recordNavigationLocation(workId, targetPath, target.line);
      const targetTab = codeWorkspace.tabs.find(
        (tab) => tab.work_id === workId && tab.path === targetPath,
      );
      if (targetTab) codeWorkspace.updateLine(targetTab.tabId, target.line);
      lmeWorkspace.updateCodeLocation(workId, targetPath, target.line);
      undertakings.setSelection({
        path: targetPath,
        line: target.line,
        entityId: null,
      });
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    }
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "p") {
      event.preventDefault();
      void showQuickOpen();
    }
    if (event.altKey && !event.metaKey && !event.ctrlKey) {
      if (event.key === "ArrowLeft" && codeWorkspace.canNavigate(workId, -1)) {
        event.preventDefault();
        void navigate(-1);
      } else if (
        event.key === "ArrowRight" &&
        codeWorkspace.canNavigate(workId, 1)
      ) {
        event.preventDefault();
        void navigate(1);
      }
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
    void reconcileExternal(primary);
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
    if (beginEditPromise) {
      await beginEditPromise;
      return;
    }
    saving = true;
    surfaceError = null;
    beginEditPromise = (async () => {
      const begun = await beginHumanAttempt(detail.id);
      undertakings.setActiveFromItem(begun.item, {
        leaseId: begun.lease.lease_id,
        leaseGeneration: begun.lease.generation,
        executorKind: "human",
      });
      await undertakings.refreshDetail();
    })();
    try {
      await beginEditPromise;
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      beginEditPromise = null;
      saving = false;
    }
  }

  async function onDraftChanged(tabIdValue: string, value: string) {
    codeWorkspace.updateDraft(tabIdValue, value);
    if (!editable && canBeginEdit) {
      await startEditing();
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
      lspClient.sync();
      const result = await lspClient.request<
        { textDocument: { uri: string } },
        CodeDocumentSymbol[] | null
      >("textDocument/documentSymbol", {
        textDocument: { uri: documentUri },
      });
      symbols = Array.isArray(result) ? result : [];
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
    lspClient.sync();
    syncProblems();
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
    const client = lspClient;
    if (!activeTab || !documentUri || !client || languageActionRunning) return;
    const cursor = editor?.getCursorPosition() ?? { line: 0, character: 0 };
    if (action === "rename" && !newName?.trim()) return;
    languageActionRunning = true;
    surfaceError = null;
    try {
      if (action === "rename" && !(await saveAll())) {
        throw new Error("Resolve unsaved files before renaming across the project");
      }
      client.sync();
      const position = { line: cursor.line, character: cursor.character };
      const [method, params] = action === "references"
        ? ["textDocument/references", {
            textDocument: { uri: documentUri },
            position,
            context: { includeDeclaration: true },
          }]
        : action === "rename"
          ? ["textDocument/rename", {
              textDocument: { uri: documentUri },
              position,
              newName: newName!.trim(),
            }]
          : action === "format"
            ? ["textDocument/formatting", {
                textDocument: { uri: documentUri },
                options: {
                  tabSize: Number(editorConventions.indent_size ?? editorConventions.tab_width) || 2,
                  insertSpaces: editorConventions.indent_style !== "tab",
                },
              }]
            : ["textDocument/codeAction", {
                textDocument: { uri: documentUri },
                range: { start: position, end: position },
                context: { diagnostics: [], only: ["source.organizeImports"] },
              }];
      const result = await client.request(method, params);
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
        void navigateLanguageLocation("definition");
        break;
      case "declaration":
        void navigateLanguageLocation("declaration");
        break;
      case "type_definition":
        void navigateLanguageLocation("typeDefinition");
        break;
      case "implementation":
        void navigateLanguageLocation("implementation");
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
    await lmeWorkspace.openCodeFile(workId, tab.path, { line: tab.line });
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
    if (!interactive || !workId) return;
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
    if (!interactive || !workId) return;
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
    if (!interactive || !id || !prepared) {
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
    if (!interactive || !reviewAvailable || !workId || !path) {
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
    const root = workspaceRoot;
    if (
      !interactive ||
      !activeTabId ||
      !root ||
      !languageSupportsLsp(activeTabLanguage)
    ) {
      lspClient = null;
      lspError = null;
      lspConnecting = false;
      return;
    }
    let cancelled = false;
    let release = () => {};
    let unregisterWorkspaceBridge = () => {};
    lspClient = null;
    lspError = null;
    lspConnecting = true;
    const cancelDeferred = deferCodeWorkspaceWork(() => {
      const lease = acquireCodeWorkspaceLspClient({
        workId,
        workspaceRoot: root,
        language: activeTabLanguage,
      });
      release = lease.release;
      unregisterWorkspaceBridge = lease.workspaceBridge.register({
        handlesUri: (uri) => Boolean(pathFromUri(uri, root)),
        requestFile: async (uri) => {
          const path = pathFromUri(uri, root);
          if (!path) return null;
          const source = await getUndertakingSource(workId, path);
          return {
            languageId: languageForWorkspacePath(path),
            text: source.content,
          };
        },
        displayFile: async (uri) => {
          const path = pathFromUri(uri, root);
          if (!path) return null;
          const source = await lmeWorkspace.openCodeFile(workId, path, {
            recordNavigation: false,
          });
          return source ? codeEditorViewRegistry.waitFor(uri) : null;
        },
      });
      void lease.client
        .then((client) => {
          if (cancelled) {
            return;
          }
          lspClient = client;
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
      unregisterWorkspaceBridge();
      release();
    };
  });

  $effect(() => {
    void activeTabId;
    void documentUri;
    void lspClient;
    let cleanup = () => {};
    untrack(() => {
      const uri = documentUri;
      const client = lspClient;
      if (!interactive || !activeTabId || !uri || !client) {
        languageCapabilities = {};
        editorConventions = {};
        return;
      }
      const cancelDeferred = deferCodeWorkspaceWork(() => {
        void refreshSymbols();
        languageCapabilities = (client.serverCapabilities ?? {}) as Record<string, unknown>;
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
    if (!interactive || !tabId) return;
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
    cursorTotalLines = 1;
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
    if (!interactive || !leaseId || generation == null) return;
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

  $effect(() => {
    if (!interactive || !activeTab) {
      codeEditorStatus.clear(statusOwnerId);
      return;
    }
    codeEditorStatus.publish(statusOwnerId, {
      workId,
      path: activeTab.path,
      line: cursorLine,
      totalLines: cursorTotalLines,
      column: cursorColumn,
      language: activeTab.language,
      indentation: indentStatusLabel,
      issueCount: Math.max(problems.length, workspaceProblemRows.length),
      dirty,
      saving,
      saveWhisper,
      control: statusControlLabel,
      languageState: lspConnecting
        ? "connecting"
        : lspError
          ? "editing-only"
          : "ready",
    });
  });

  $effect(() => {
    if (!interactive) return;
    const onShowProblems = () => void showProblems();
    window.addEventListener("medousa-code-show-problems", onShowProblems);
    return () => {
      window.removeEventListener("medousa-code-show-problems", onShowProblems);
    };
  });

  onDestroy(() => {
    if (linePersistTimer) clearTimeout(linePersistTimer);
    codeEditorStatus.clear(statusOwnerId);
  });
</script>

<section class="flex flex-col overflow-hidden rounded-lg border border-surface-500/35 bg-surface-950/45 {fill ? 'min-h-0 flex-1' : 'min-h-[26rem]'}">
  {#if activeTab}
    <header class="relative z-20 flex shrink-0 items-center justify-between gap-2 border-b border-surface-500/30 bg-surface-950/65 px-1.5 py-0.5">
      <div class="flex min-w-0 flex-1 items-center gap-1.5">
        <div class="flex shrink-0 items-center">
          <button type="button" class="scripts-workbench-toolbar-btn" aria-label="Go back" title="Go back" disabled={!codeWorkspace.canNavigate(workId, -1)} onclick={() => void navigate(-1)}><ArrowLeft size={14} strokeWidth={1.75} /></button>
          <button type="button" class="scripts-workbench-toolbar-btn" aria-label="Go forward" title="Go forward" disabled={!codeWorkspace.canNavigate(workId, 1)} onclick={() => void navigate(1)}><ArrowRight size={14} strokeWidth={1.75} /></button>
        </div>
        <CodeBreadcrumbs
          path={activeTab.path}
          symbols={symbolTrail}
          onPathSegment={onBreadcrumbPath}
          onSymbol={(line) => editor?.revealLine(line)}
        />
      </div>
      <div class="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          class="scripts-workbench-toolbar-btn {contextPanel === 'problems' ? 'scripts-workbench-toolbar-btn-active' : ''}"
          title="Issues"
          aria-label="Show issues"
          aria-pressed={contextPanel === "problems"}
          onclick={() => void showProblems()}
        ><CircleAlert size={14} strokeWidth={1.75} /></button>
        <button
          type="button"
          class="scripts-workbench-toolbar-btn {contextPanel === 'outline' ? 'scripts-workbench-toolbar-btn-active' : ''}"
          title={titleWithShortcut("Structure", "code-structure")}
          aria-label="Show file structure"
          aria-pressed={contextPanel === "outline"}
          disabled={!lspClient}
          onclick={() => void showOutline()}
        ><ListTree size={14} strokeWidth={1.75} /></button>
        {#if selectedTask}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn {runningTask ? 'scripts-workbench-toolbar-btn-active' : ''}"
            title={runningTask ? `Stop ${selectedTask.label}` : `${selectedTask.label}: ${selectedTask.argv.join(" ")}`}
            aria-label={runningTask ? `Stop ${selectedTask.label}` : `Run ${selectedTask.label}`}
            onclick={() => {
              if (runningTask) void stopDetectedTask();
              else void runDetectedTask();
            }}
          >{#if runningTask}<X size={14} />{:else}<Play size={14} strokeWidth={1.75} />{/if}</button>
        {/if}
        {#if terminalAvailable}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn {terminalDockOpen ? 'scripts-workbench-toolbar-btn-active' : ''}"
            title={titleWithShortcut("Toggle terminal", "code-terminal")}
            aria-label="Toggle terminal"
            aria-pressed={terminalDockOpen}
            onclick={() => void toggleTerminalDock()}
          ><SquareTerminal size={14} strokeWidth={1.75} /></button>
        {/if}
        <button
          type="button"
          class="scripts-workbench-toolbar-btn {worldOpen ? 'scripts-workbench-toolbar-btn-active' : ''}"
          title="Understand this code"
          aria-label="Understand this code"
          aria-pressed={worldOpen}
          onclick={onToggleWorld}
        ><Orbit size={14} strokeWidth={1.75} /></button>
        {#if reviewAvailable}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn text-amber-300/80"
            title="Review changes"
            aria-label="Review changes"
            onclick={onOpenReview}
          ><GitPullRequestArrow size={14} strokeWidth={1.75} /></button>
        {/if}

        <span class="mx-0.5 h-4 w-px shrink-0 bg-surface-500/35" aria-hidden="true"></span>

        <button
          type="button"
          class="scripts-workbench-toolbar-btn"
          title={titleWithShortcut("Find", "code-find")}
          aria-label="Find in file"
          onclick={() => editor?.openFind()}
        ><Search size={14} strokeWidth={1.75} /></button>
        <details class="relative">
          <summary class="scripts-workbench-toolbar-btn cursor-pointer list-none [&::-webkit-details-marker]:hidden" title="Editor options" aria-label="Editor options">•••</summary>
          <div class="absolute right-0 top-full z-30 mt-1 w-44 rounded-md border border-surface-500/40 bg-surface-900 p-1 shadow-xl">
            <button type="button" class="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-[10px] text-content-secondary hover:bg-surface-800" onclick={toggleWordWrap}>
              <span>Word wrap</span>
              <span class="text-content-quiet">{wordWrap ? "On" : "Off"}</span>
            </button>
            <button type="button" class="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-[10px] text-content-secondary hover:bg-surface-800" onclick={toggleLineNumbers}>
              <span>Line numbers</span>
              <span class="text-content-quiet">{showLineNumbers ? "On" : "Off"}</span>
            </button>
            <button type="button" class="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-[10px] text-content-secondary hover:bg-surface-800" onclick={cycleFontSize}>
              <span>Font size</span>
              <span class="text-content-quiet">{fontSize}px</span>
            </button>
            <button type="button" class="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-[10px] text-content-secondary hover:bg-surface-800" onclick={cycleTabSize}>
              <span>Tab size</span>
              <span class="text-content-quiet">{tabSizePref}</span>
            </button>
            {#if projectTasks.length > 1}
              <label class="block border-t border-surface-500/20 px-2 pb-1.5 pt-1">
                <span class="text-[9px] uppercase tracking-wider text-content-quiet">Project command</span>
                <select class="mt-1 w-full rounded bg-surface-800 px-1.5 py-1 text-[10px] text-content-secondary outline-none" aria-label="Project command" bind:value={selectedTaskId}>
                  {#each projectTasks as task (task.id)}
                    <option value={task.id}>{task.label}</option>
                  {/each}
                </select>
              </label>
            {/if}
            {#if projectTasks.some((task) => task.kind === "test")}
              <button type="button" class="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-[10px] text-content-secondary hover:bg-surface-800" onclick={() => void toggleTests()}>
                <span>Discovered tests</span>
                <span class="text-content-quiet">{testsOpen ? "Hide" : "Show"}</span>
              </button>
            {/if}
            {#if canFormat && editable}
              <button type="button" class="flex w-full items-center rounded px-2 py-1.5 text-left text-[10px] text-content-secondary hover:bg-surface-800 disabled:opacity-40" disabled={languageActionRunning} onclick={() => void runLanguageAction("format")}>Format document</button>
            {/if}
            {#if canCodeAction && editable}
              <button type="button" class="flex w-full items-center rounded px-2 py-1.5 text-left text-[10px] text-content-secondary hover:bg-surface-800 disabled:opacity-40" disabled={languageActionRunning} onclick={() => void runLanguageAction("organize_imports")}>Organize imports</button>
            {/if}
            {#if lspError}
              <button type="button" class="flex w-full items-center rounded px-2 py-1.5 text-left text-[10px] text-content-warning hover:bg-surface-800 disabled:opacity-40" disabled={repairingLanguage} onclick={() => void repairLanguageSupport()}>{repairingLanguage ? "Repairing…" : "Repair language support"}</button>
            {/if}
          </div>
        </details>
        <button
          type="button"
          class="scripts-workbench-toolbar-btn"
          disabled={activeTab.loading || saving}
          onclick={() => void reload()}
          aria-label="Reload file"
          title="Reload file"
        ><RotateCcw size={14} strokeWidth={1.75} /></button>
        {#if agentHasControl}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={saving}
            onclick={() => void reclaimHuman()}
            aria-label="Resume editing"
            title="Resume editing — take the file back from the agent"
          ><UserRound size={14} strokeWidth={1.75} /></button>
        {:else if !editable && canBeginEdit}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={saving}
            onclick={() => void startEditing()}
            aria-label="Edit file"
            title="Start editing (or just type)"
          ><Pencil size={14} strokeWidth={1.75} /></button>
        {:else}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={!editable || !dirty || saving}
            onclick={() => void save()}
            aria-label="Save file"
            title={saving ? "Saving…" : titleWithShortcut("Save", "code-save")}
          ><Save size={14} strokeWidth={1.75} /></button>
        {/if}
      </div>
    </header>

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

    <div class="flex min-h-0 flex-1 overflow-hidden">
      <div class="relative min-h-0 min-w-0 flex-1">
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
              languageId={editorTab.language}
              {documentUri}
              lspLanguageId={editorTab.language}
              client={lspClient}
              readOnly={!bufferInteractive}
              contentSyncKey={editorTab.syncKey}
              changedLines={reviewChangedLines}
              conventionIndentStyle={editorConventions.indent_style ?? null}
              conventionTabSize={Number.parseInt(editorConventions.indent_size ?? editorConventions.tab_width ?? "", 10) || null}
              {wordWrap}
              {showLineNumbers}
              onchange={(value) => void onDraftChanged(editorTab.tabId, value)}
              onCursorChanged={(cursor) => handleCursorChanged(editorTab, cursor)}
              onSelectionChanged={(selection) => (editorSelection = selection.text ? selection : null)}
              onProblemsChanged={syncProblems}
              onContextMenu={onEditorContextMenu}
              onLanguageNavigationRequested={(kind) => void navigateLanguageLocation(kind)}
            />
          {/key}
        {:else if !activeTab.loading}
          <div class="flex h-full min-h-48 items-center justify-center p-6 text-xs text-content-quiet">
            This file is not plain text, so Medousa cannot edit it here.
          </div>
        {/if}
      </div>
    </div>

    {#if contextPanel}
      <div class="max-h-44 shrink-0 overflow-y-auto border-t border-surface-500/30 bg-surface-950/80">
        <div class="sticky top-0 z-10 flex items-center justify-between border-b border-surface-500/25 bg-surface-950 px-2 py-1">
          <span class="text-[9px] font-medium uppercase tracking-wider text-content-tertiary">
            {contextPanel === "problems" ? "Issues" : contextPanel === "references" ? "Uses" : "Structure"}
          </span>
          <button type="button" class="rounded p-0.5 text-content-quiet hover:text-surface-200" aria-label="Close context panel" onclick={() => (contextPanel = null)}><X size={11} /></button>
        </div>
        {#if contextPanel === "problems"}
          {#if problems.length === 0 && workspaceProblemRows.length === 0}
            <p class="px-3 py-3 text-[10px] text-content-quiet">No issues found in this project.</p>
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
                <span class="min-w-0 flex-1 text-[10px] text-content-secondary"><span class="text-content-quiet">{problem.path} · </span>{problem.message}</span>
                <span class="shrink-0 font-mono text-[9px] text-content-quiet">{problem.line}</span>
              </button>
            {/each}
          {/if}
        {:else if contextPanel === "references"}
          {#if references.length === 0}
            <p class="px-3 py-3 text-[10px] text-content-quiet">No other uses found.</p>
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
                  await lmeWorkspace.openCodeFile(workId, referencePath, {
                    line: referenceLine,
                  });
                  undertakings.setSelection({ path: referencePath, line: referenceLine, entityId: null });
                  await tick();
                  editor?.revealLine(referenceLine);
                }}
              >
                <FileCode2 size={11} class="shrink-0 text-content-link/70" />
                <span class="min-w-0 flex-1 truncate text-[10px] text-content-secondary">{referencePath ?? reference.uri}</span>
                <span class="font-mono text-[9px] text-content-quiet">{referenceLine}</span>
              </button>
            {/each}
          {/if}
        {:else if symbolsLoading}
          <p class="px-3 py-3 text-[10px] text-content-quiet">Reading file structure…</p>
        {:else if symbols.length === 0}
          <p class="px-3 py-3 text-[10px] text-content-quiet">No structure is available for this file.</p>
        {:else}
          {#each symbols as symbol (`${symbol.name}:${symbolLine(symbol)}`)}
            <button
              type="button"
              class="flex w-full items-center gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
              onclick={() => editor?.revealLine(symbolLine(symbol))}
            >
              <ListTree size={11} class="shrink-0 text-content-link/70" />
              <span class="min-w-0 flex-1 truncate text-[10px] text-content-secondary">{symbol.name}</span>
              <span class="font-mono text-[9px] text-content-quiet">{symbolLine(symbol)}</span>
            </button>
          {/each}
        {/if}
      </div>
    {/if}

    {#if taskResult}
      <div class="shrink-0 border-t {taskResult.success ? 'border-emerald-500/25 bg-emerald-950/20 text-emerald-200' : 'border-rose-500/30 bg-rose-950/25 text-rose-200'}">
      <button type="button" class="flex w-full items-center justify-between gap-2 px-2.5 py-1 text-left text-[9px]" title="Run this check again" onclick={() => void runDetectedTask()}>
        <span>{taskResult.success ? "Passed" : "Needs attention"} · {taskResult.task.label}</span>
        <span class="text-current">Rerun · {(taskResult.duration_ms / 1000).toFixed(1)}s{taskResult.exit_code != null ? ` · exit ${taskResult.exit_code}` : ""}</span>
      </button>
      {#each taskResult.locations.slice(0, 5) as location (`${location.path}:${location.line}:${location.column}`)}
        <button type="button" class="flex w-full items-center gap-2 border-t border-current/10 px-2.5 py-1 text-left text-[9px] hover:bg-white/5" onclick={() => void openTaskLocation(location.path, location.line)}>
          <span class="min-w-0 flex-1 truncate">{location.message || location.path}</span>
          <span class="shrink-0 font-mono">{location.path}:{location.line}</span>
        </button>
      {/each}
      </div>
    {/if}
    {#if testsOpen}
      <div class="max-h-44 shrink-0 overflow-y-auto border-t border-surface-500/25 bg-surface-950/90">
        <div class="sticky top-0 flex items-center justify-between bg-surface-950 px-2.5 py-1 text-[9px] uppercase tracking-wider text-content-quiet"><span>Project tests</span><span>{projectTests.length}</span></div>
        {#if projectTests.length === 0}
          <p class="px-3 py-3 text-[10px] text-content-quiet">No individual tests were discovered. The project test command still works.</p>
        {:else}
          {#each projectTests as test (test.id)}
            <div class="flex items-center border-t border-surface-500/15">
              <button type="button" class="min-w-0 flex-1 truncate px-3 py-1.5 text-left text-[10px] text-content-secondary hover:bg-surface-800/60" onclick={() => void openTaskLocation(test.path, test.line)}>{test.label}<span class="ml-2 font-mono text-[9px] text-content-faint">{test.path}:{test.line}</span></button>
              <button type="button" class="mr-2 rounded px-1.5 py-0.5 text-[9px] text-content-link hover:bg-surface-800 disabled:opacity-40" disabled={runningTask} onclick={() => void runDetectedTask(test)}>Run</button>
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
            <FileCode2 size={24} class="mx-auto text-content-faint" />
            <p class="mt-2 text-xs font-medium text-content-secondary">Set up this project</p>
            <p class="mt-1 text-[10px] leading-relaxed text-content-quiet">
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
              >Open file</button>
              {#if terminalAvailable}
                <button
                  type="button"
                  class="rounded border border-surface-500/40 px-3 py-1.5 text-[11px] text-surface-200 hover:bg-surface-800"
                  disabled={dockBusy}
                  onclick={() => void toggleTerminalDock(true)}
                >Terminal</button>
              {/if}
            </div>
          {:else}
            <FileCode2 size={24} class="mx-auto text-content-faint" />
            <p class="mt-2 text-xs font-medium text-content-secondary">Open a file</p>
            <p class="mt-1 text-[10px] leading-relaxed text-content-quiet">
              Jump in with Quick Open, or pick a path from the project tree.
            </p>
            <div class="mt-3 flex flex-wrap items-center justify-center gap-2">
              <button
                type="button"
                class="rounded bg-primary-500/80 px-3 py-1.5 text-[11px] font-medium text-surface-50"
                onclick={() => void showQuickOpen()}
              >Open file</button>
              {#if terminalAvailable}
                <button
                  type="button"
                  class="rounded border border-surface-500/40 px-3 py-1.5 text-[11px] text-surface-200 hover:bg-surface-800"
                  disabled={dockBusy}
                  onclick={() => void toggleTerminalDock(true)}
                >Terminal</button>
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

{#if quickOpen}
  <div class="fixed inset-0 z-[120] flex items-start justify-center px-4 pt-[12vh]">
    <button type="button" class="absolute inset-0 bg-black/35" aria-label="Close file picker" onclick={() => (quickOpen = false)}></button>
    <div class="relative w-full max-w-xl overflow-hidden rounded-lg border border-surface-500/50 bg-surface-950 shadow-2xl" role="dialog" aria-modal="true" aria-label="Open a file" tabindex="-1">
      <div class="flex items-center gap-2 border-b border-surface-500/30 px-3">
        <Search size={14} class="text-content-quiet" />
        <input bind:this={quickInput} class="min-w-0 flex-1 bg-transparent py-2.5 text-sm text-surface-100 outline-none" placeholder="File, @symbol, or :line" bind:value={quickQuery} oninput={() => { quickIndex = 0; void refreshQuickSymbols(); }} onkeydown={(event) => {
          if (event.key === "ArrowDown") { event.preventDefault(); quickIndex = Math.min(quickIndex + 1, quickResultCount - 1); }
          if (event.key === "ArrowUp") { event.preventDefault(); quickIndex = Math.max(quickIndex - 1, 0); }
          if (event.key === "Enter") { event.preventDefault(); chooseQuickResult(); }
        }} />
        <span class="text-[9px] text-content-faint">⌘P</span>
      </div>
      <div class="max-h-[50vh] overflow-y-auto py-1">
        {#if quickMode === "line"}
          <button type="button" class="flex w-full items-center gap-2 px-3 py-2 text-left text-content-secondary hover:bg-surface-800" onclick={chooseQuickLine}>
            <span class="font-mono text-xs text-content-link">:{quickQuery.slice(1).trim() || "line"}</span>
            <span class="text-[10px] text-content-quiet">Go to a line in {activeTab?.title}</span>
          </button>
        {:else if quickMode === "symbol" && quickSymbolResults.length === 0}
          <p class="px-3 py-3 text-xs text-content-quiet">No matching project symbols.</p>
        {:else if quickMode === "symbol"}
          {#each quickSymbolResults as symbol, index (`${symbol.name}:${symbol.location?.uri}:${symbol.location?.range?.start?.line}`)}
            <button type="button" class="flex w-full items-center gap-2 px-3 py-1.5 text-left {index === quickIndex ? 'bg-surface-800 text-surface-100' : 'text-content-tertiary hover:bg-surface-900'}" onmouseenter={() => (quickIndex = index)} onclick={() => void chooseQuickSymbol(symbol)}>
              <ListTree size={12} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-xs">{symbol.name}</span>
              <span class="min-w-0 max-w-[60%] truncate font-mono text-[9px] text-content-faint">{symbol.containerName ?? pathFromUri(symbol.location?.uri) ?? ""}</span>
            </button>
          {/each}
        {:else if quickLoading}
          <p class="px-3 py-3 text-xs text-content-quiet">Reading project files…</p>
        {:else if quickResults.length === 0}
          <p class="px-3 py-3 text-xs text-content-quiet">No matching files.</p>
        {:else}
          {#each quickResults as file, index (file.path)}
            <button type="button" class="flex w-full items-center gap-2 px-3 py-1.5 text-left {index === quickIndex ? 'bg-surface-800 text-surface-100' : 'text-content-tertiary hover:bg-surface-900'}" onmouseenter={() => (quickIndex = index)} onclick={() => void chooseQuickFile(file)}>
              <FileCode2 size={12} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-xs">{file.path.split("/").pop()}</span>
              <span class="min-w-0 max-w-[60%] truncate font-mono text-[9px] text-content-faint">{file.path}</span>
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
    {@const conflictFiles = [{
      path: conflictTab.path,
      status: "changed",
      hunks: buildTextDiff(projectVersion.content, conflictTab.draft),
    }]}
    <div class="fixed inset-0 z-[125] flex items-center justify-center p-4">
      <button type="button" class="absolute inset-0 bg-black/55" aria-label="Close comparison" onclick={() => (comparingTabId = null)}></button>
      <div class="relative flex max-h-[85vh] w-full max-w-5xl flex-col overflow-hidden rounded-lg border border-surface-500/50 bg-surface-950 shadow-2xl" role="dialog" aria-modal="true" aria-label="Compare file versions" tabindex="-1">
        <header class="flex items-center justify-between border-b border-surface-500/30 px-3 py-2">
          <div>
            <p class="text-xs font-medium text-surface-100">Choose the version to continue with</p>
            <p class="font-mono text-[9px] text-content-quiet">{conflictTab.path}</p>
            <p class="mt-0.5 text-[10px] text-content-quiet">Draft vs project — same comparison chrome as Review.</p>
          </div>
          <button type="button" class="rounded p-1 text-content-quiet hover:text-surface-100" aria-label="Close comparison" onclick={() => (comparingTabId = null)}><X size={13} /></button>
        </header>
        <div class="min-h-0 flex-1 overflow-auto px-3 py-2">
          <DiffStack files={conflictFiles} mode="side" showJumpList={false} />
        </div>
        <footer class="flex justify-end gap-2 border-t border-surface-500/30 px-3 py-2">
          <button type="button" class="rounded px-2 py-1 text-[10px] text-content-secondary hover:bg-surface-800" onclick={() => useProjectVersion(conflictTab)}>Use project version</button>
          <button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-[10px] font-medium text-white" onclick={() => keepDraft(conflictTab)}>Keep my draft</button>
        </footer>
      </div>
    </div>
  {/if}
{/if}

<CodeEditorContextMenu
  open={editorMenuOpen}
  x={editorMenuX}
  y={editorMenuY}
  canDefinition={canDefinition}
  canDeclaration={canDeclaration}
  canTypeDefinition={canTypeDefinition}
  canImplementation={canImplementation}
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
        <p class="text-[10px] text-content-quiet">Applies across the project when the language server supports it.</p>
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
        <button type="button" class="rounded px-2 py-1 text-[10px] text-content-tertiary hover:bg-surface-800" onclick={cancelInlineRename}>Cancel</button>
        <button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-[10px] font-medium text-white disabled:opacity-40" disabled={!renameDraft.trim() || languageActionRunning} onclick={() => void commitInlineRename()}>Rename</button>
      </footer>
    </div>
  </div>
{/if}

<svelte:window
  onfocus={() => {
    if (interactive) reconcileOpenFiles();
  }}
  onkeydown={(event) => {
    if (!interactive) return;
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
      if (activeTab && codeWorkspace.isDirty(activeTab)) void saveTab(activeTab);
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
