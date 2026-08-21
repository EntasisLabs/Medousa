<script lang="ts">
  import { onDestroy, tick, untrack, type Snippet } from "svelte";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import { buildTextDiff } from "$lib/diff/buildTextDiff";
  import CodeEditorContextMenu, {
    type CodeEditorMenuAction,
  } from "$lib/components/code/CodeEditorContextMenu.svelte";
  import CodeQuickOpenModal from "$lib/components/code/CodeQuickOpenModal.svelte";
  import CodeEditorChrome from "$lib/components/code/CodeEditorChrome.svelte";
  import CodeEditorWorkspace from "$lib/components/code/CodeEditorWorkspace.svelte";
  import CodeEditorDialogs from "$lib/components/code/CodeEditorDialogs.svelte";
  import { openTrackedTerminal } from "$lib/utils/undertakingWorkspace";
  import { writeToTerminal } from "$lib/terminal/terminalInputBridge";
  import {
    findCodeLanguageMatrixEntry,
    getCodeEditorConventions,
    getCodeLanguageMatrix,
    type CodeDocumentSymbol,
  } from "$lib/code/codingEngineClient";
  import { CodeLspSession } from "$lib/code/codeLspSession.svelte";
  import { CodeChangesController } from "$lib/code/codeChangesController.svelte";
  import { CodeProblemsController } from "$lib/code/codeProblemsController.svelte";
  import { CodeQuickOpenController } from "$lib/code/codeQuickOpenController.svelte";
  import { CodeSaveController } from "$lib/code/codeSaveController.svelte";
  import { CodeTasksController } from "$lib/code/codeTasksController.svelte";
  import {
    pathToFileUri,
    workspaceRelativePathFromUri,
  } from "$lib/code/codeDocumentUri";
  import {
    buildCodeWorkspaceEditPlan,
    type CodeWorkspaceEditPlan,
  } from "$lib/code/codeWorkspaceEdit";
  import {
    CodeProjectEventStream,
    planOpenBufferAction,
    watchedFileChangesForProjectEvent,
    type ForgeProjectEvent,
  } from "$lib/code/codeProjectEvents";
  import {
    codeWorkbenchState,
  } from "$lib/code/codeWorkbenchState.svelte";
  import { handleCodeEditorWindowKeydown } from "$lib/code/codeEditorWindowKeys";
  import { codeEditorViewRegistry } from "$lib/code/codeEditorViewRegistry";
  import type { CodeLanguageNavigationKind } from "$lib/code/codeLanguageNavigation";
  import { containingSymbolTrail } from "$lib/code/codeDocumentSymbols";
  import {
    codeEditorLspLanguageId,
    languageRepairPackageId,
    languageSupportsLsp,
    resolveCodeEditorLanguage,
  } from "$lib/code/codeEditorLanguageRegistry";
  import {
    canInvokeCodeSaveShortcut,
    CODE_SAVE_NO_LEASE_ERROR,
  } from "$lib/code/codeSaveGate";
  import {
    canStartHumanEditing,
    startHumanEditingSession,
    applyUndertakingSourceWorkspaceEdit,
    getUndertakingSource,
    heartbeatLease,
    isMissingForgeRoute,
    saveUndertakingSources,
    getReviewFile,
    type ForgeSourceFile,
  } from "$lib/code/codeDocumentService";
  import { codeWorkspace, type CodeDocumentTab } from "$lib/stores/codeWorkspace.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { setActiveCodeInsights } from "$lib/utils/undertakingWorkspace";
  import { deferCodeWorkspaceWork } from "$lib/utils/codeWorkspaceTrace";
  import { fetchPackagesCatalog, installPackage } from "$lib/utils/packagesApi";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
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
    /** Leading project identity for the unified chrome bar. */
    projectTitle?: string | null;
    phaseLabel?: string | null;
    agentRunning?: boolean;
    agentLabel?: string;
    onToggleWorld?: () => void;
    onOpenReview?: () => void;
    onOpenTerminal?: () => void;
    onStopAgent?: () => void;
    onResumeEditing?: () => void;
    onProvision?: () => Promise<void>;
    preferredAgent?: "codex" | "cursor";
    onHandoffToAgent?: (runtime: "codex" | "cursor", draft?: string) => Promise<void>;
    onReclaimHuman?: () => Promise<void>;
    /** Project kebab items (Ask Codex, Terminal, Discard, …). */
    projectMenu?: Snippet;
  }

  let {
    fill = false,
    workId: boundWorkId = "",
    resourcePath = null,
    interactive = true,
    worldOpen = false,
    reviewAvailable = false,
    terminalAvailable = false,
    projectTitle = null,
    phaseLabel = null,
    agentRunning = false,
    agentLabel = "agent",
    onToggleWorld,
    onOpenReview,
    onOpenTerminal,
    onStopAgent,
    onResumeEditing,
    onProvision,
    preferredAgent = "codex",
    onHandoffToAgent,
    onReclaimHuman,
    projectMenu,
  }: Props = $props();

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
  let lspSession = new CodeLspSession();
  const lspClient = $derived(lspSession.client);
  const lspError = $derived(lspSession.error);
  const lspConnecting = $derived(lspSession.connecting);
  const lspStatus = $derived(lspSession.status);
  const languageMatrix = $derived(lspSession.languageMatrix);
  const languageMatrixError = $derived(lspSession.languageMatrixError);
  let repairingLanguage = $state(false);
  let languageActionRunning = $state(false);
  let symbols = $state<CodeDocumentSymbol[]>([]);
  let symbolsLoading = $state(false);
  let searchOpen = $state(false);
  let externalVersions = $state<Record<string, ForgeSourceFile>>({});
  let comparingTabId = $state<string | null>(null);
  let reviewChangedLines = $state<Array<{ line: number; kind: string }>>([]);
  let languageCapabilities = $state<Record<string, unknown>>({});
  let editorConventions = $state<{ indent_style?: "space" | "tab"; indent_size?: string; tab_width?: string }>({});
  let references = $state<Array<{ uri?: string; range?: { start?: { line?: number } } }>>([]);
  let cursorLine = $state(1);
  let cursorTotalLines = $state(1);
  let cursorColumn = $state(1);
  let editorSelection = $state<{
    startLine: number;
    endLine: number;
    text: string;
  } | null>(null);
  let linePersistTimer: ReturnType<typeof setTimeout> | null = null;
  let projectEventStream: CodeProjectEventStream | null = null;
  let treeRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  let editorMenuOpen = $state(false);
  let editorMenuX = $state(0);
  let editorMenuY = $state(0);
  let renameOpen = $state(false);
  let renameDraft = $state("");
  let renameInput = $state<HTMLInputElement | null>(null);
  let refactorPreview = $state<{
    workId: string;
    plan: CodeWorkspaceEditPlan;
  } | null>(null);
  let refactorApplying = $state(false);
  let refactorDiffMode = $state<"inline" | "side">("side");
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
  const activeLspLanguage = $derived(codeEditorLspLanguageId(activeTabLanguage));
  const activeTabLine = $derived(activeTab?.line ?? null);
  const dirty = $derived(Boolean(activeTab && codeWorkspace.isDirty(activeTab)));
  const previewOnly = $derived(Boolean(activeTab?.preview));
  const editable = $derived(
    Boolean(
      interactive &&
      !previewOnly &&
      context?.workId === workId &&
      context.leaseId &&
      context.leaseGeneration != null,
    ),
  );
  const agentHasControl = $derived(
    Boolean(
      context?.executorKind &&
        context.executorKind !== "human" &&
        context.leaseId &&
        context.leaseGeneration != null,
    ),
  );
  const canBeginEdit = $derived(
    Boolean(
      interactive &&
        !previewOnly &&
        !agentHasControl &&
        canStartHumanEditing(detail?.allowed_actions),
    ),
  );
  /** Soft lease: typing is allowed when a human attempt can begin or continue. */
  const bufferInteractive = $derived((editable || canBeginEdit) && !previewOnly);
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
  const workspaceProblemLanguages = $derived(
    [...new Set(
      tabs
        .map((tab) => codeEditorLspLanguageId(tab.language))
        .filter((language) => languageSupportsLsp(language)),
    )].sort(),
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
  const refactorDiffFiles = $derived(
    (refactorPreview?.plan.files ?? []).map((file) => ({
      id: file.id,
      path: file.path,
      oldPath: file.oldPath,
      status:
        file.status === "created"
          ? "added"
          : file.status === "modified"
            ? "changed"
            : file.status,
      hunks: buildTextDiff(file.before, file.after),
    })),
  );

  const changes = new CodeChangesController({
    getWorkId: () => workId,
    persistOpen: (open) => {
      if (!workId) return;
      codeWorkbenchState.setChangesOpen(workId, open);
      codeWorkspace.scheduleLayoutPersist(workId);
    },
    ensureLease: () => ensureHumanLease(),
    onError: (message) => {
      surfaceError = message || null;
    },
    onFilesMutated: () => {
      reconcileOpenFiles();
      void quick.refreshTree();
    },
    refreshDetail: () => undertakings.refreshDetail(),
    openReview: (id, title) => lmeWorkspace.openCodeReview(id, title),
    getReviewTitle: () => detail?.title ?? "project",
  });
  const tasks = new CodeTasksController({
    getWorkId: () => workId,
    persistTestsOpen: (open) => {
      if (!workId) return;
      setFeedbackPanel(open ? "tests" : null);
    },
    persistOutputOpen: (open) => {
      if (!workId) return;
      setFeedbackPanel(open ? "output" : null);
    },
    persistSelectedTask: (taskId) => {
      if (!workId) return;
      codeWorkbenchState.setPrimaryTask(workId, taskId);
      codeWorkspace.scheduleLayoutPersist(workId);
    },
    persistRunRefs: (activeRunId, recentRunIds) => {
      if (!workId) return;
      codeWorkbenchState.setTaskRuns(workId, activeRunId, recentRunIds);
      codeWorkspace.scheduleLayoutPersist(workId);
    },
    prepareRun: () => save.saveAll(),
    ensureLease: () => ensureHumanLease(),
    onError: (message) => {
      surfaceError = message || null;
    },
    onOpenTerminal: () => onOpenTerminal?.(),
    refreshDetail: () => undertakings.refreshDetail(),
  });
  const problems = new CodeProblemsController({
    getWorkId: () => workId,
    getWorkspaceRoot: () => workspaceRoot,
    getDocumentUri: () => documentUri,
    getActiveLanguage: () => activeTab?.language ?? "",
    getWorkspaceLanguages: () => workspaceProblemLanguages,
    persistPanel: (panel) => {
      if (!workId) return;
      codeWorkbenchState.setContextPanel(workId, panel);
      codeWorkspace.scheduleLayoutPersist(workId);
    },
    openProblem: async (problem) => {
      await openTaskLocation(problem.path, problem.line);
    },
    onError: (message) => {
      surfaceError = message || null;
    },
    syncDocument: () => {
      lspClient?.sync();
      void tick().then(() => {
        problems.setDocumentProblems(editor?.getProblems() ?? []);
      });
    },
    onProblemsSelected: (selected) => {
      if (selected) setFeedbackPanel("problems");
      else if (feedbackPanel === "problems") setFeedbackPanel(null);
    },
  });
  const feedbackPanel = $derived.by(() => {
    if (terminalDockOpen) return "terminal" as const;
    if (tasks.testsOpen) return "tests" as const;
    if (tasks.outputOpen) return "output" as const;
    if (problems.panel === "problems") return "problems" as const;
    return null;
  });
  let lastFailedRunPresented = "";

  function setFeedbackPanel(
    panel: "problems" | "output" | "tests" | "terminal" | null,
    persist = true,
  ) {
    tasks.restoreOutputOpen(panel === "output");
    tasks.restoreTestsOpen(panel === "tests");
    terminalDockOpen = panel === "terminal";
    if (panel === "problems") problems.restorePanel("problems");
    else if (problems.panel === "problems") problems.restorePanel(null);
    if (!persist || !workId) return;
    codeWorkbenchState.setBottomPanel(workId, panel);
    codeWorkspace.scheduleLayoutPersist(workId);
  }

  async function selectFeedbackPanel(
    panel: "problems" | "output" | "tests" | "terminal",
  ) {
    if (panel === "problems") {
      if (problems.panel !== "problems") await problems.showProblems();
      else setFeedbackPanel("problems");
      return;
    }
    if (panel === "output") {
      tasks.toggleOutput(true);
      return;
    }
    if (panel === "tests") {
      if (!tasks.testsOpen) await tasks.toggleTests();
      else setFeedbackPanel("tests");
      return;
    }
    await toggleTerminalDock(true);
  }
  const quick = new CodeQuickOpenController({
    getWorkId: () => workId,
    getLspClient: () => lspClient,
    pathFromUri: (uri) => pathFromUri(uri),
    onError: (message) => {
      surfaceError = message || null;
    },
    onShown: () => {},
    openFile: async (filePath, line) => {
      const tab = await lmeWorkspace.openCodeFile(workId, filePath, { line });
      undertakings.setSelection({ path: filePath, line, entityId: null });
      await tick();
      if (!tab) return;
      if (line > 1) editor?.revealLine(line);
      else editor?.focusEditor();
    },
    revealLine: (line) => {
      editor?.revealLine(line);
      if (activeTab) {
        codeWorkspace.updateLine(activeTab.tabId, line);
        lmeWorkspace.updateCodeLocation(activeTab.work_id, activeTab.path, line);
        undertakings.setSelection({ path: activeTab.path, line, entityId: null });
      }
    },
  });
  const save = new CodeSaveController({
    getWorkId: () => workId,
    getContext: () => context,
    getDetail: () => detail,
    getActiveTab: () => activeTab,
    getActiveTabId: () => activeTabId,
    getTabs: () => tabs,
    getEditor: () => editor,
    getEditable: () => editable,
    getCanBeginEdit: () => canBeginEdit,
    ensureLease: () => ensureHumanLease(),
    onError: (message) => {
      surfaceError = message;
    },
    captureEditorContext: () => captureEditorContext(),
    preferredAgent: () => preferredAgent,
    onHandoffToAgent: async (runtime, draft) => { await onHandoffToAgent?.(runtime, draft); },
    onReclaimHuman: async () => { await onReclaimHuman?.(); },
    updateDraft: (tabId, value) => codeWorkspace.updateDraft(tabId, value),
    isDirty: (tab) => codeWorkspace.isDirty(tab as CodeDocumentTab),
    acceptSaved: (tabId, source) => codeWorkspace.acceptSaved(tabId, source),
    setTabError: (tabId, message) => codeWorkspace.setError(tabId, message),
    setActiveFromItem: (item, lease) => undertakings.setActiveFromItem(item, lease),
    refreshDetail: () => undertakings.refreshDetail(),
  });
  const busy = $derived(save.busy);

  function syncProblems() {
    void tick().then(() => {
      problems.setDocumentProblems(editor?.getProblems() ?? []);
    });
  }

  function toggleSearch(forceOpen?: boolean) {
    const next =
      forceOpen === true ? true : forceOpen === false ? false : !searchOpen;
    searchOpen = next;
    if (!workId) return;
    codeWorkbenchState.setSearchOpen(workId, next);
    codeWorkspace.scheduleLayoutPersist(workId);
  }

  async function ensureHumanLease(): Promise<{ leaseId: string; generation: number }> {
    let leaseId = context?.leaseId ?? null;
    let generation = context?.leaseGeneration ?? null;
    if ((!leaseId || generation == null) && detail && canStartHumanEditing(detail.allowed_actions)) {
      const begun = await startHumanEditingSession(detail.id, detail.allowed_actions);
      leaseId = begun.lease.lease_id;
      generation = begun.lease.generation;
      undertakings.setActiveFromItem(begun.item, {
        leaseId,
        leaseGeneration: generation,
        executorKind: "human",
      });
    }
    if (!leaseId || generation == null) {
      throw new Error(CODE_SAVE_NO_LEASE_ERROR);
    }
    return { leaseId, generation };
  }

  async function openSearchHit(path: string, line: number) {
    await lmeWorkspace.openCodeFile(workId, path, {
      line,
      groupId: shellTabs.activeGroupId,
    });
    undertakings.setSelection({ path, line, entityId: null });
    await tick();
    editor?.revealLine(line);
  }

  async function openTaskLocation(path: string, line: number) {
    await lmeWorkspace.openCodeFile(workId, path, { line });
    undertakings.setSelection({ path, line, entityId: null });
    await tick();
    editor?.revealLine(line);
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

  function openSyntaxThemeSettings() {
    settingsNav.openSection("preferences");
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
      diagnostics: problems.effective.slice(0, 20).map((problem) =>
        `${problem.path}:${problem.line}:${problem.character} ${problem.message}`
      ),
      last_verification: tasks.result
        ? `${tasks.result.task.label}: ${tasks.result.success ? "passed" : "failed"}${tasks.result.exit_code != null ? ` (exit ${tasks.result.exit_code})` : ""}`
        : null,
    });
  }

  async function toggleTerminalDock(forceOpen?: boolean) {
    const next = forceOpen === true ? true : forceOpen === false ? false : !terminalDockOpen;
    if (!next) {
      setFeedbackPanel(null);
      return;
    }
    if (!detail || !terminalAvailable) {
      surfaceError = "Terminal is not available for this project yet.";
      return;
    }
    setFeedbackPanel("terminal");
    const runSessionId = tasks.run?.session_id?.trim();
    if (runSessionId) {
      dockSessionId = runSessionId;
      undertakings.bindTerminal(runSessionId);
      return;
    }
    if (dockSessionId) return;
    dockBusy = true;
    surfaceError = null;
    try {
      const sessionId = await openTrackedTerminal(detail, { activate: false });
      dockSessionId = sessionId;
      if (!sessionId) surfaceError = "Could not open a workshop shell for this project.";
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
      setFeedbackPanel(null);
    } finally {
      dockBusy = false;
    }
  }

  async function runSelectedTextInTerminal() {
    const text = editorSelection?.text?.trim();
    if (!text) {
      surfaceError = "Select text in the editor to run in the Terminal.";
      return;
    }
    await toggleTerminalDock(true);
    await tick();
    if (!writeToTerminal(text, workId)) {
      surfaceError = "Open the Terminal dock, then run the selection again.";
    }
  }

  async function popOutTerminal() {
    if (!detail) return;
    const runSessionId = tasks.run?.session_id?.trim();
    setFeedbackPanel(null);
    if (runSessionId) {
      undertakings.bindTerminal(runSessionId);
      shellTabs.openTerminal(runSessionId, {
        activate: true,
        title: `Task · ${tasks.run?.task.label ?? detail.title}`,
        workId,
      });
      return;
    }
    await openTrackedTerminal(detail, { activate: true });
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
    if (resolved === "plaintext") {
      return codeEditorLspLanguageId(activeTabLanguage);
    }
    return codeEditorLspLanguageId(resolved);
  }

  async function navigate(direction: -1 | 1) {
    const result = await codeWorkspace.navigate(workId, direction);
    if (!result) return;
    const { tab, entry } = result;
    if (
      entry.groupId &&
      shellTabs.groups.some((group) => group.id === entry.groupId)
    ) {
      shellTabs.focusGroup(entry.groupId);
    }
    await lmeWorkspace.openCodeFile(workId, tab.path, {
      line: tab.line,
      recordNavigation: false,
      groupId: entry.groupId ?? shellTabs.activeGroupId,
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
        shellTabs.activeGroupId,
      );
      codeWorkspace.recordNavigationLocation(
        workId,
        targetPath,
        target.line,
        shellTabs.activeGroupId,
      );
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

  function openLanguagePackages() {
    settingsNav.openSection("packages");
    shellTabs.openDestination("settings");
    layout.openShellSidebarView("settings");
  }

  async function showLanguageLogs() {
    await showLanguagePanel();
  }

  function restartLanguageServer() {
    lspSession.restart();
  }

  const activeLanguageMatrix = $derived(
    findCodeLanguageMatrixEntry(languageMatrix, activeLspLanguage),
  );

  async function refreshLanguageMatrix(options?: { quiet?: boolean }) {
    if (!languageSupportsLsp(activeTabLanguage)) {
      lspSession.languageMatrix = [];
      lspSession.languageMatrixError = null;
      return;
    }
    try {
      lspSession.languageMatrix = await getCodeLanguageMatrix();
      lspSession.languageMatrixError = null;
    } catch (err) {
      if (!options?.quiet) {
        lspSession.languageMatrixError =
          err instanceof Error ? err.message : String(err);
      }
    }
  }

  async function repairLanguageSupport() {
    if (!isCoLocatedWorkshop()) {
      openLanguagePackages();
      return;
    }
    repairingLanguage = true;
    surfaceError = null;
    try {
      await refreshLanguageMatrix({ quiet: true });
      const catalog = await fetchPackagesCatalog();
      if (!catalog) throw new Error("Package repair is unavailable here");
      const matrixPackage = activeLanguageMatrix?.packageId ?? null;
      const languagePackage =
        matrixPackage ?? languageRepairPackageId(activeTabLanguage);
      if (!languagePackage) {
        openLanguagePackages();
        surfaceError = activeLanguageMatrix?.command
          ? `Install ${activeLanguageMatrix.command} on this workshop, then restart the language server.`
          : "This language has no Medousa package yet — install its language server on the workshop.";
        return;
      }
      const wanted = ["coding-engine", languagePackage].filter(
        (id, index, list) => list.indexOf(id) === index,
      );
      for (const packageId of wanted) {
        const row = catalog.packages.find((entry) => entry.id === packageId);
        if (row && !row.installed) await installPackage(packageId);
      }
      await refreshLanguageMatrix({ quiet: true });
      lspSession.restart();
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
    } catch (err) {
      const status = (err as { status?: number } | null)?.status;
      if (status === 404) {
        await reconcileExternalDelete(tab.work_id, tab.path);
      }
      // Tree refresh and explicit reload surface durable errors. Polling stays quiet.
    }
  }

  function reconcileOpenFiles() {
    if (!workId) return;
    for (const tab of codeWorkspace.tabsFor(workId)) {
      void reconcileExternal(tab);
    }
  }

  function scheduleTreeRefresh() {
    if (!workId) return;
    if (treeRefreshTimer) clearTimeout(treeRefreshTimer);
    treeRefreshTimer = setTimeout(() => {
      treeRefreshTimer = null;
      void quick.refreshTree();
    }, 250);
  }

  function notifyLanguageServerOfProjectEvent(event: ForgeProjectEvent) {
    const client = lspClient;
    const root = workspaceRoot;
    if (!client || !root) return;
    const fileUri = (path: string) =>
      pathToFileUri(`${root.replace(/[\\/]$/, "")}/${path}`);
    if (event.kind === "created" && event.path) {
      client.notification("workspace/didCreateFiles", {
        files: [{ uri: fileUri(event.path) }],
      });
    } else if (event.kind === "renamed" && event.path && event.old_path) {
      client.notification("workspace/didRenameFiles", {
        files: [{ oldUri: fileUri(event.old_path), newUri: fileUri(event.path) }],
      });
    } else if (event.kind === "deleted" && event.path) {
      client.notification("workspace/didDeleteFiles", {
        files: [{ uri: fileUri(event.path) }],
      });
    }
    const changes = watchedFileChangesForProjectEvent(event, fileUri);
    if (changes.length) {
      client.notification("workspace/didChangeWatchedFiles", { changes });
      clientSyncAfterRefactor();
    }
  }

  async function reconcileExternalRename(oldPath: string, newPath: string) {
    if (!workId) return;
    const tab = codeWorkspace.tabsFor(workId).find((entry) => entry.path === oldPath);
    if (!tab) {
      scheduleTreeRefresh();
      return;
    }
    const dirty = codeWorkspace.isDirty(tab);
    const draft = tab.draft;
    const line = tab.line ?? 1;
    const wasActive = activeTabPath === oldPath;
    try {
      const source = await getUndertakingSource(workId, newPath);
      codeWorkspace.replacePath(workId, oldPath, source);
      if (dirty) {
        const next = codeWorkspace.tabsFor(workId).find((entry) => entry.path === newPath);
        if (next) {
          codeWorkspace.updateDraft(next.tabId, draft);
          externalVersions = { ...externalVersions, [next.tabId]: source };
          codeWorkspace.setError(
            next.tabId,
            "This file was renamed in the project. Your draft is safe.",
          );
        }
      }
      await lmeWorkspace.replaceCodeFile(workId, oldPath, newPath, line, {
        activate: wasActive,
      });
      if (wasActive) {
        undertakings.setSelection({ path: newPath, line, entityId: null });
      }
    } catch {
      await reconcileExternalDelete(workId, oldPath);
    }
    scheduleTreeRefresh();
  }

  async function reconcileExternalDelete(targetWorkId: string, path: string) {
    const tab = codeWorkspace.tabsFor(targetWorkId).find((entry) => entry.path === path);
    if (!tab) {
      scheduleTreeRefresh();
      return;
    }
    if (codeWorkspace.isDirty(tab)) {
      codeWorkspace.setError(
        tab.tabId,
        "This file was deleted in the project. Your draft is safe.",
      );
      scheduleTreeRefresh();
      return;
    }
    const wasActive = activeTabPath === path;
    codeWorkspace.removePath(targetWorkId, path);
    await lmeWorkspace.closeCodeFile(targetWorkId, path);
    if (wasActive) {
      undertakings.setSelection({ path: null, line: null, entityId: null });
    }
    scheduleTreeRefresh();
  }

  async function handleProjectEvent(event: ForgeProjectEvent) {
    if (!workId || event.work_id !== workId) return;
    notifyLanguageServerOfProjectEvent(event);
    changes.scheduleRefresh();
    const plan = planOpenBufferAction(event);
    switch (plan.action) {
      case "reconcile": {
        const tab = codeWorkspace
          .tabsFor(workId)
          .find((entry) => entry.path === plan.path);
        if (tab) void reconcileExternal(tab);
        if (event.kind === "created") scheduleTreeRefresh();
        break;
      }
      case "rename":
        await reconcileExternalRename(plan.oldPath, plan.newPath);
        break;
      case "delete":
        await reconcileExternalDelete(workId, plan.path);
        break;
      case "reconcile_all":
        reconcileOpenFiles();
        scheduleTreeRefresh();
        break;
      default:
        break;
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

  async function showOutline() {
    const next = problems.panel === "outline" ? null : "outline";
    problems.setPanel(next);
    if (next === "outline") await refreshSymbols();
  }

  async function showLanguagePanel() {
    const next = problems.panel === "language" ? null : "language";
    problems.setPanel(next);
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

  function sourceWasNotFound(err: unknown): boolean {
    const status = (err as { status?: number } | null)?.status;
    const message = err instanceof Error ? err.message : String(err ?? "");
    return status === 404 || /HTTP\s+404\b/i.test(message);
  }

  async function reviewWorkspaceEdit(result: unknown): Promise<boolean> {
    const root = workspaceRoot;
    if (!root) throw new Error("The project workspace root is unavailable.");
    const plan = await buildCodeWorkspaceEditPlan(result, {
      workspaceRoot: root,
      loadSource: async (path) => {
        try {
          return await getUndertakingSource(workId, path);
        } catch (err) {
          if (sourceWasNotFound(err)) return null;
          throw err;
        }
      },
    });
    if (plan.operations.length === 0) return false;
    refactorDiffMode = "side";
    refactorPreview = { workId, plan };
    return true;
  }

  async function applyTextOnlyRefactorFallback(
    plan: CodeWorkspaceEditPlan,
    lease: { leaseId: string; generation: number },
  ): Promise<ForgeSourceFile[]> {
    if (plan.operations.some((operation) => operation.kind !== "write")) {
      throw new Error(
        "This workshop needs a newer Medousa daemon to apply refactors that create, rename, or delete files.",
      );
    }
    const digests = new Map(
      plan.preconditions
        .filter((precondition) => precondition.kind === "existing")
        .map((precondition) => [precondition.path, precondition.expected_digest]),
    );
    const finalContent = new Map<string, string>();
    for (const operation of plan.operations) {
      if (operation.kind === "write") finalContent.set(operation.path, operation.content);
    }
    const files = [...finalContent].map(([path, content]) => {
      const expectedDigest = digests.get(path);
      if (!expectedDigest) {
        throw new Error(`The refactor is missing a source snapshot for ${path}.`);
      }
      return { path, content, expected_digest: expectedDigest };
    });
    return saveUndertakingSources(workId, {
      files,
      lease_id: lease.leaseId,
      generation: lease.generation,
    });
  }

  function notifyLanguageServerOfRefactor(plan: CodeWorkspaceEditPlan) {
    const client = lspClient;
    const root = workspaceRoot;
    if (!client || !root) return;
    const fileUri = (path: string) =>
      pathToFileUri(`${root.replace(/[\\/]$/, "")}/${path}`);
    // Notify the server about the net filesystem result, not intermediate
    // transaction paths. This avoids reporting an overwrite destination as
    // deleted after a source identity has already moved into that same path.
    const deletedPaths = new Set(
      plan.files.filter((file) => file.status === "deleted").map((file) => file.path),
    );
    const finalPaths = new Set(
      plan.files
        .filter((file) => file.status !== "deleted")
        .map((file) => file.path),
    );
    const replacedPaths = new Set(
      [...deletedPaths].filter((path) =>
        plan.files.some((file) => file.status === "created" && file.path === path),
      ),
    );
    const created = plan.files
      .filter((file) => file.status === "created" && !replacedPaths.has(file.path))
      .map((file) => ({ uri: fileUri(file.path) }));
    const renamed = plan.files
      .filter((file) => file.status === "renamed" && file.oldPath)
      .map((file) => ({
        oldUri: fileUri(file.oldPath!),
        newUri: fileUri(file.path),
      }));
    const deleted = [...deletedPaths]
      .filter((path) => !finalPaths.has(path))
      .map((path) => ({ uri: fileUri(path) }));
    if (created.length) client.notification("workspace/didCreateFiles", { files: created });
    if (renamed.length) client.notification("workspace/didRenameFiles", { files: renamed });
    if (deleted.length) client.notification("workspace/didDeleteFiles", { files: deleted });
    const watchedChanges = plan.files.flatMap((file) => {
      if (file.status === "modified" || replacedPaths.has(file.path)) {
        return [{ uri: fileUri(file.path), type: 2 }];
      }
      if (file.status === "created") return [{ uri: fileUri(file.path), type: 1 }];
      if (file.status === "deleted" && !finalPaths.has(file.path)) {
        return [{ uri: fileUri(file.path), type: 3 }];
      }
      if (file.status === "renamed" && file.oldPath) {
        return [
          { uri: fileUri(file.oldPath), type: 3 },
          { uri: fileUri(file.path), type: 1 },
        ];
      }
      return [];
    });
    const watchedByUri = new Map(watchedChanges.map((change) => [change.uri, change]));
    if (watchedByUri.size) {
      client.notification("workspace/didChangeWatchedFiles", {
        changes: [...watchedByUri.values()],
      });
    }
  }

  async function reconcileAppliedRefactor(
    plan: CodeWorkspaceEditPlan,
    saved: ForgeSourceFile[],
  ) {
    const initiallyActivePath = activeTabPath;
    const openTabs = [...tabs];
    const openByPath = new Map(openTabs.map((tab) => [tab.path, tab]));
    const savedByPath = new Map(saved.map((source) => [source.path, source]));
    const affectedTabIds = new Set(openTabs
      .filter((tab) => plan.preconditions.some((entry) => entry.path === tab.path))
      .map((tab) => tab.tabId));

    // An overwrite rename may delete an open destination before moving the
    // source identity into that same path, so close replaced identities first.
    for (const file of plan.files.filter((entry) => entry.status === "deleted")) {
      if (!openByPath.has(file.path)) continue;
      codeWorkspace.removePath(workId, file.path);
      await lmeWorkspace.closeCodeFile(workId, file.path);
    }
    await tick();
    for (const file of plan.files.filter((entry) => entry.status === "deleted")) {
      const presentationStillOpen = lmeWorkspace.tabs.some(
        (tab) =>
          tab.kind === "code" &&
          tab.workId === workId &&
          tab.resource.kind === "file" &&
          tab.resource.path === file.path,
      );
      if (presentationStillOpen) await lmeWorkspace.closeCodeFile(workId, file.path);
    }

    for (const file of plan.files) {
      const oldPath = file.oldPath;
      if (file.status !== "renamed" || !oldPath) continue;
      const oldTab = openByPath.get(oldPath);
      const source = savedByPath.get(file.path);
      if (!oldTab || !source) continue;
      codeWorkspace.replacePath(workId, oldPath, source);
      await lmeWorkspace.replaceCodeFile(
        workId,
        oldPath,
        file.path,
        oldTab.line ?? 1,
        { activate: initiallyActivePath === oldPath },
      );
      if (initiallyActivePath === oldPath) {
        undertakings.setSelection({ path: file.path, line: oldTab.line, entityId: null });
      }
    }

    if (
      plan.files.some(
        (file) => file.status === "deleted" && file.path === initiallyActivePath,
      ) &&
      !plan.files.some(
        (file) => file.status === "renamed" && file.oldPath === initiallyActivePath,
      )
    ) {
      undertakings.setSelection({ path: null, line: null, entityId: null });
    }

    for (const source of saved) {
      const tab = codeWorkspace.tabs.find(
        (entry) => entry.work_id === workId && entry.path === source.path,
      );
      if (tab) codeWorkspace.acceptSaved(tab.tabId, source);
    }
    const nextExternal = { ...externalVersions };
    for (const tabId of affectedTabIds) delete nextExternal[tabId];
    externalVersions = nextExternal;
    notifyLanguageServerOfRefactor(plan);
    clientSyncAfterRefactor();
    try {
      await quick.refreshTree();
    } catch {
      // The applied transaction is authoritative; the normal tree refresh can retry.
    }
    await undertakings.refreshDetail();
  }

  function clientSyncAfterRefactor() {
    void tick().then(() => lspClient?.sync());
  }

  async function applyRefactorPreview() {
    const preview = refactorPreview;
    const active = context;
    if (!preview || preview.workId !== workId || refactorApplying) return;
    if (!active?.leaseId || active.leaseGeneration == null) {
      surfaceError = "Editing control changed. Reopen the rename preview and try again.";
      return;
    }
    refactorApplying = true;
    surfaceError = null;
    try {
      let saved: ForgeSourceFile[];
      try {
        saved = await applyUndertakingSourceWorkspaceEdit(workId, {
          preconditions: preview.plan.preconditions,
          operations: preview.plan.operations,
          lease_id: active.leaseId,
          generation: active.leaseGeneration,
        });
      } catch (err) {
        if (!isMissingForgeRoute(err)) throw err;
        saved = await applyTextOnlyRefactorFallback(preview.plan, {
          leaseId: active.leaseId,
          generation: active.leaseGeneration,
        });
      }
      await reconcileAppliedRefactor(preview.plan, saved);
      refactorPreview = null;
      save.flashWhisper("Refactor applied", 1800);
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      refactorApplying = false;
    }
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
      if (action === "rename" && !(await save.saveAll())) {
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
        problems.setPanel("references");
        return;
      }
      if (action === "rename" && await reviewWorkspaceEdit(result)) return;
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

  function containingSymbol(): string | null {
    return symbolTrail[symbolTrail.length - 1]?.name ?? null;
  }

  $effect(() => {
    if (!interactive || !workId) return;
    setActiveCodeInsights(workId, {
      // Cursor/symbol context is captured only at an explicit handoff boundary.
      containing_symbol: null,
      diagnostics: problems.effective.slice(0, 20).map((problem) =>
        `${problem.path}:${problem.line}:${problem.character} ${problem.message}`
      ),
      last_verification: tasks.result
        ? `${tasks.result.task.label}: ${tasks.result.success ? "passed" : "failed"}${tasks.result.exit_code != null ? ` (exit ${tasks.result.exit_code})` : ""}`
        : null,
    });
  });

  /** Keep matcher diagnostics scoped to the selected run; LSP problems stay intact. */
  $effect(() => {
    const run = tasks.run;
    if (!run) {
      problems.setTaskRun(null);
      return;
    }
    const result = run.result ?? tasks.result;
    problems.setTaskRun({
      runId: run.run_id,
      taskLabel: run.task.label,
      success: result?.success ?? null,
      locations: tasks.liveLocations,
    });
    if (
      result?.success === false &&
      tasks.liveLocations.length > 0 &&
      lastFailedRunPresented !== run.run_id
    ) {
      lastFailedRunPresented = run.run_id;
      void selectFeedbackPanel("problems");
    }
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
    return tasks.bindTaskList(id, prepared, interactive);
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
    const root = workspaceRoot;
    const uri = documentUri;
    if (
      !interactive ||
      !activeTabId ||
      !workId ||
      !root ||
      !uri ||
      !languageSupportsLsp(activeTabLanguage)
    ) {
      lspSession.stop();
      return;
    }
    const language = activeLspLanguage;
    const languageLabel = activeTabLanguage;
    const scope = `${workId}:${language}:${uri}`;
    // connect() is a no-op reconnect when scope is unchanged and already live,
    // but always safe to call — it cancels in-flight work via generation.
    lspSession.connect({
      workId,
      workspaceRoot: root,
      language,
      languageLabel,
      documentUri: uri,
      bridge: {
        handlesUri: (candidateUri) => Boolean(pathFromUri(candidateUri, root)),
        requestFile: async (candidateUri) => {
          const path = pathFromUri(candidateUri, root);
          if (!path) return null;
          const source = await getUndertakingSource(workId, path);
          return {
            languageId: languageForWorkspacePath(path),
            text: source.content,
          };
        },
        displayFile: async (candidateUri) => {
          const path = pathFromUri(candidateUri, root);
          if (!path) return null;
          const source = await lmeWorkspace.openCodeFile(workId, path, {
            recordNavigation: false,
          });
          return source ? codeEditorViewRegistry.waitFor(candidateUri) : null;
        },
      },
    });
  });

  $effect(() => {
    return () => lspSession.dispose();
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
        void getCodeEditorConventions({ workId, uri, language: activeLspLanguage })
          .then((conventions) => (editorConventions = conventions))
          .catch(() => (editorConventions = {}));
      });
      cleanup = cancelDeferred;
    });
    return cleanup;
  });

  $effect(() => {
    const scope = problems.scopeKey;
    const languagesKey = workspaceProblemLanguages.join("\u0000");
    const showingProblems = problems.panel === "problems";
    void languagesKey;
    void lspClient;
    if (!interactive || !scope) {
      untrack(() => {
        if (problems.workspaceScope) void problems.refresh({ quiet: true });
      });
      return;
    }
    const refreshTimer = setTimeout(
      () => untrack(() => void problems.refresh({ quiet: !showingProblems })),
      showingProblems ? 0 : 350,
    );
    if (!showingProblems) return () => clearTimeout(refreshTimer);
    const pollingTimer = setInterval(
      () => untrack(() => void problems.refresh({ quiet: true })),
      2_000,
    );
    return () => {
      clearTimeout(refreshTimer);
      clearInterval(pollingTimer);
    };
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
    problems.setDocumentProblems([]);
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
      issueCount: problems.counts.total,
      dirty,
      saving: busy,
      saveWhisper: save.saveWhisper,
      control: statusControlLabel,
      execution: tasks.running
        ? `${tasks.run?.task.label ?? "Task"} · ${tasks.run?.state === "ready" ? "ready" : tasks.run?.state === "stopping" ? "stopping" : "running"}`
        : tasks.result
          ? `${tasks.result.task.label} · ${tasks.result.success ? "passed" : "failed"}`
          : null,
      languageState: lspStatus.phase === "failed"
        ? "failed"
        : lspStatus.phase === "reconnecting"
          ? "reconnecting"
          : lspConnecting
            ? "connecting"
            : lspError
              ? "editing-only"
              : "ready",
      languageDetail: (() => {
        if (lspStatus.progress) {
          const pct =
            lspStatus.progress.percentage != null
              ? ` ${Math.round(lspStatus.progress.percentage)}%`
              : "";
          const msg = lspStatus.progress.message
            ? ` · ${lspStatus.progress.message}`
            : "";
          return `${lspStatus.progress.title}${msg}${pct}`;
        }
        if (lspStatus.phase === "reconnecting" || lspStatus.phase === "failed") {
          return lspStatus.detail;
        }
        if (lspStatus.notice) return lspStatus.notice;
        if (lspConnecting) return "Language starting…";
        return null;
      })(),
    });
  });

  $effect(() => {
    if (!interactive) return;
    const onShowProblems = () => void problems.showProblems();
    const onCodeCommand = (event: Event) => {
      const id = (event as CustomEvent<{ id?: string }>).detail?.id;
      if (!id) return;
      switch (id) {
        case "workbench.action.quickOpen":
          void quick.show();
          break;
        case "workbench.action.navigateBack":
          void navigate(-1);
          break;
        case "workbench.action.navigateForward":
          void navigate(1);
          break;
        case "workbench.actions.view.problems":
          void problems.showProblems();
          break;
        case "workbench.action.terminal.toggleTerminal":
          void toggleTerminalDock();
          break;
        case "workbench.action.terminal.focusFind":
          void (async () => {
            await toggleTerminalDock(true);
            await tick();
            window.dispatchEvent(new CustomEvent("medousa-terminal-find"));
          })();
          break;
        case "workbench.action.terminal.runSelectedText":
          void runSelectedTextInTerminal();
          break;
        case "workbench.view.testing":
          void tasks.toggleTests();
          break;
        case "workbench.action.tasks.runPrimary":
        case "workbench.action.tasks.runTask":
          void tasks.runDetected();
          break;
        case "workbench.action.tasks.build":
          void tasks.runKind("build");
          break;
        case "workbench.action.tasks.test":
          void tasks.runKind("test");
          break;
        case "workbench.action.tasks.verify":
          void tasks.runKind("verify");
          break;
        case "workbench.action.tasks.rerunLast":
          void tasks.rerunLast();
          break;
        case "workbench.action.tasks.terminate":
          void tasks.stopDetected();
          break;
        case "workbench.action.findInFiles":
          toggleSearch(true);
          break;
        case "workbench.view.scm":
          void changes.toggle(true);
          break;
        case "git.fetch":
          void changes.toggle(true).then(() => changes.runSync("fetch"));
          break;
        case "git.pull":
          void changes.toggle(true).then(() => changes.runSync("pull"));
          break;
        case "git.push":
          void changes.toggle(true).then(() => changes.runSync("push"));
          break;
        case "git.sync":
          void changes.toggle(true).then(() => changes.runSync("sync"));
          break;
        case "medousa.forge.checkpoint":
          void changes.toggle(true).then(() => changes.sealForReview());
          break;
        case "git.viewHistory":
          void changes.toggle(true).then(() => {
            if (!changes.historyOpen) void changes.toggleHistory();
          });
          break;
        case "git.blame.toggle":
          void changes.toggle(true).then(() => changes.toggleBlame());
          break;
        case "workbench.action.output.toggleOutput":
          tasks.toggleOutput();
          break;
        default:
          break;
      }
    };
    window.addEventListener("medousa-code-show-problems", onShowProblems);
    window.addEventListener("medousa-code-command", onCodeCommand);
    return () => {
      window.removeEventListener("medousa-code-show-problems", onShowProblems);
      window.removeEventListener("medousa-code-command", onCodeCommand);
    };
  });

  /** Restore contextual Code regions once per undertaking open. */
  $effect(() => {
    if (!interactive || !workId) return;
    const id = workId;
    void (async () => {
      await codeWorkspace.hydrate(id);
      if (workId !== id) return;
      const layout = codeWorkbenchState.layoutFor(id);
      problems.restorePanel(layout.context_panel === "problems" ? null : layout.context_panel);
      const restoredFeedback = tasks.running ? "output" : layout.bottom_panel;
      setFeedbackPanel(restoredFeedback, false);
      tasks.restoreSelectedTask(layout.primary_task);
      tasks.restoreRunRefs(layout.active_run, layout.recent_runs);
      void tasks.hydrateTaskRuns(id);
      searchOpen = layout.search;
      changes.restoreOpen(layout.changes);
      if (restoredFeedback === "terminal") void toggleTerminalDock(true);
      if (layout.changes) void changes.refresh();
    })();
  });

  $effect(() => {
    if (!interactive) {
      projectEventStream?.stop();
      return;
    }
    const id = workId;
    if (!projectEventStream) {
      projectEventStream = new CodeProjectEventStream({
        onEvent: (event) => void handleProjectEvent(event),
      });
    }
    projectEventStream.setWorkId(id || null);
  });

  onDestroy(() => {
    if (linePersistTimer) clearTimeout(linePersistTimer);
    if (treeRefreshTimer) clearTimeout(treeRefreshTimer);
    projectEventStream?.teardown();
    projectEventStream = null;
    changes.dispose();
    tasks.dispose();
    save.dispose();
    codeEditorStatus.clear(statusOwnerId);
  });
</script>

<section class="code-source-editor flex flex-col overflow-hidden {fill ? 'code-source-editor--fill min-h-0 flex-1' : 'min-h-[26rem] rounded-lg border border-surface-500/35 bg-surface-950/45'}">
  <CodeEditorChrome
    {workId}
    {activeTab}
    {projectTitle}
    {phaseLabel}
    {reviewAvailable}
    {onOpenReview}
    {agentRunning}
    {agentLabel}
    {onStopAgent}
    {onResumeEditing}
    {problems}
    {searchOpen}
    onToggleSearch={() => toggleSearch()}
    {changes}
    {terminalAvailable}
    {terminalDockOpen}
    onToggleTerminal={() => void toggleTerminalDock()}
    {tasks}
    {agentHasControl}
    {busy}
    {editable}
    {canBeginEdit}
    {dirty}
    onReclaimHuman={() => void save.reclaimHuman()}
    onStartEditing={() => void save.startEditing()}
    onSave={() => void save.save()}
    savingFile={save.savingFile}
    onOpenFind={() => editor?.openFind()}
    hasLspClient={Boolean(lspClient)}
    onShowOutline={() => void showOutline()}
    {onToggleWorld}
    {worldOpen}
    onReload={() => void reload()}
    {wordWrap}
    onToggleWordWrap={toggleWordWrap}
    {showLineNumbers}
    onToggleLineNumbers={toggleLineNumbers}
    {fontSize}
    onCycleFontSize={cycleFontSize}
    onOpenSyntaxTheme={openSyntaxThemeSettings}
    {tabSizePref}
    onCycleTabSize={cycleTabSize}
    {canFormat}
    {canCodeAction}
    {languageActionRunning}
    onLanguageAction={(action) => void runLanguageAction(action)}
    onRestartLanguage={restartLanguageServer}
    onShowLanguageLogs={() => void showLanguageLogs()}
    {lspError}
    {repairingLanguage}
    onRepairLanguage={() => void repairLanguageSupport()}
    {projectMenu}
    onNavigate={(direction) => void navigate(direction)}
    onPathSegment={onBreadcrumbPath}
  />

  <CodeEditorWorkspace
    {workId}
    {activeTab}
    {surfaceError}
    {landError}
    {needsProvision}
    {onProvision}
    {externalVersions}
    bind:editor
    bind:editorSelection
    {editorPrefsEpoch}
    {documentUri}
    {lspClient}
    {bufferInteractive}
    {reviewChangedLines}
    {editorConventions}
    {wordWrap}
    {showLineNumbers}
    {save}
    {busy}
    {agentHasControl}
    {onHandoffToAgent}
    {problems}
    {canReference}
    {canRename}
    {editable}
    {languageActionRunning}
    onLanguageAction={(action) => void runLanguageAction(action)}
    onBeginRename={() => beginInlineRename()}
    onCursorChanged={handleCursorChanged}
    onProblemsChanged={syncProblems}
    onContextMenu={onEditorContextMenu}
    onLanguageNavigation={(kind) => void navigateLanguageLocation(kind)}
    {symbols}
    {symbolsLoading}
    {references}
    {activeTabLanguage}
    {activeLspLanguage}
    {activeLanguageMatrix}
    {pathFromUri}
    onRestartLanguage={restartLanguageServer}
    onOpenLocation={(path, line) => void openTaskLocation(path, line)}
    {tasks}
    {searchOpen}
    onOpenSearchHit={(path, line) => void openSearchHit(path, line)}
    onToggleSearch={(open) => toggleSearch(open)}
    onSearchApplied={async () => {
      await undertakings.refreshDetail();
      reconcileOpenFiles();
      await quick.refreshTree();
    }}
    {quick}
    {changes}
    bind:comparingTabId
    onUseProjectVersion={useProjectVersion}
    onKeepDraft={keepDraft}
    {dockSessionId}
    workspaceRoot={workspaceRoot ?? context?.worktree ?? null}
    terminalTitle={detail?.title?.trim() || "Terminal"}
    {terminalAvailable}
    {dockBusy}
    onToggleTerminal={(forceOpen) => void toggleTerminalDock(forceOpen)}
    onPopOutTerminal={() => void popOutTerminal()}
    {feedbackPanel}
    onSelectFeedbackPanel={(panel) => void selectFeedbackPanel(panel)}
    onCloseFeedbackPanel={() => setFeedbackPanel(null)}
  />

</section>

<style>
  .code-source-editor--fill {
    border: 0;
    border-radius: 0;
    background: transparent;
  }
</style>

<CodeQuickOpenModal {quick} activeTitle={activeTab?.title} {pathFromUri} />

<CodeEditorDialogs
  {workId}
  bind:comparingTabId
  {tabs}
  {externalVersions}
  bind:refactorPreview
  {refactorApplying}
  {refactorDiffFiles}
  bind:refactorDiffMode
  {surfaceError}
  {renameOpen}
  bind:renameDraft
  {languageActionRunning}
  bind:renameInput
  onUseProjectVersion={useProjectVersion}
  onKeepDraft={keepDraft}
  onApplyRefactor={() => void applyRefactorPreview()}
  onCancelRename={cancelInlineRename}
  onCommitRename={() => void commitInlineRename()}
/>

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

<svelte:window
  onfocus={() => {
    if (interactive) reconcileOpenFiles();
  }}
  onkeydown={(event) => {
    handleCodeEditorWindowKeydown(event, {
      interactive,
      refactorOpen: Boolean(refactorPreview),
      refactorApplying,
      clearRefactorPreview: () => { refactorPreview = null; },
      renameOpen,
      quickOpen: quick.open,
      editorMenuOpen,
      editable,
      canBeginEdit,
      canRename,
      hasActiveTab: Boolean(activeTab),
      isActiveDirty: Boolean(activeTab && codeWorkspace.isDirty(activeTab)),
      problemsPanelOpen: Boolean(problems.panel),
      canNavigate: (direction) => codeWorkspace.canNavigate(workId, direction),
      showQuickOpen: () => void quick.show(),
      closeQuickOpen: () => quick.close(),
      navigate: (direction) => void navigate(direction),
      reopenClosedTab: () => void reopenClosedTab(),
      saveAll: () => void save.saveAll(),
      saveActive: () => { if (activeTab) void save.saveTab(activeTab); },
      canSaveShortcut: canInvokeCodeSaveShortcut({ editable, canBeginEdit }),
      toggleTerminal: () => void toggleTerminalDock(),
      openSearch: () => toggleSearch(true),
      showOutline: () => void showOutline(),
      beginRename: () => beginInlineRename(),
      clearProblemsPanel: () => problems.setPanel(null),
    });
  }}
/>
