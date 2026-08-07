<script lang="ts">
  import { onDestroy, tick, untrack, type Snippet } from "svelte";
  import {
    CircleAlert,
    ArrowLeft,
    ArrowRight,
    FileCode2,
    FolderKanban,
    ListTree,
    LoaderCircle,
    Pencil,
    RotateCcw,
    Save,
    Settings2,
    Square,
    SquareTerminal,
    GitPullRequestArrow,
    Play,
    Sparkles,
    UserRound,
    X,
    Search,
    GitBranch,
  } from "@lucide/svelte";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import CodeBreadcrumbs from "$lib/components/code/CodeBreadcrumbs.svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { buildTextDiff } from "$lib/diff/buildTextDiff";
  import CodeEditorContextMenu, {
    type CodeEditorMenuAction,
  } from "$lib/components/code/CodeEditorContextMenu.svelte";
  import CodeTerminalDock from "$lib/components/work/CodeTerminalDock.svelte";
  import CodeWorkspaceSearch from "$lib/components/code/CodeWorkspaceSearch.svelte";
  import CodeChangesPanel from "$lib/components/code/CodeChangesPanel.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { openTrackedTerminal } from "$lib/utils/undertakingWorkspace";
  import { fuzzyMatchPaths } from "$lib/utils/pathFuzzyMatch";
  import { writeToTerminal } from "$lib/terminal/terminalInputBridge";
  import { resolveTaskPreviewOpenUrl } from "$lib/code/taskPreviewUrl";
  import { openInBrowser } from "$lib/utils/openInBrowser";
  import {
    findCodeLanguageMatrixEntry,
    getAllCodeWorkspaceDiagnostics,
    getCodeEditorConventions,
    getCodeLanguageMatrix,
    getCodeLanguageSessions,
    isPermanentLanguageServiceError,
    type CodeDocumentSymbol,
    type CodeLanguageSessionSnapshot,
    type CodeWorkspaceSymbol,
  } from "$lib/code/codingEngineClient";
  import { CodeLspSession } from "$lib/code/codeLspSession.svelte";
  import {
    countCodeProblems,
    filterCodeProblems,
    groupCodeProblems,
    normalizeCodeWorkspaceProblems,
    type CodeProblem,
    type CodeProblemSeverityFilter,
  } from "$lib/code/codeProblems";
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
    CodeTaskRunEventStream,
    type ProjectTaskOutputEvent,
  } from "$lib/code/codeTaskRunEvents";
  import {
    codeWorkbenchState,
    type CodeContextPanel,
  } from "$lib/code/codeWorkbenchState.svelte";
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
    CODE_SAVE_PREVIEW_ERROR,
    decideCodeSave,
  } from "$lib/code/codeSaveGate";
  import {
    canStartHumanEditing,
    startHumanEditingSession,
    applyUndertakingSourceWorkspaceEdit,
    getUndertakingSource,
    heartbeatLease,
    humanizeForgeMessage,
    isMissingForgeRoute,
    saveUndertakingSource,
    saveUndertakingSources,
    getUndertakingSourceTree,
    type ForgeSourceTreeFile,
    getProjectTasks,
    getProjectTests,
    getForgeChanges,
    getChangesFile,
    restoreChangesFile,
    fetchChanges,
    pullChanges,
    pushChanges,
    syncChanges,
    checkpointChanges,
    getChangesHistory,
    getChangesBlame,
    resolveChangesConflict,
    revertChangesHunk,
    startProjectTaskRun,
    getProjectTaskRun,
    cancelProjectTaskRun,
    getReviewFile,
    type ProjectTask,
    type ProjectTaskResult,
    type ProjectTaskRun,
    type ProjectTest,
    type ForgeChanges,
    type ChangesFileDiff,
    type ChangesHistoryEntry,
    type ChangesBlameHunk,
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
  import { codeSyntaxThemePreference } from "$lib/stores/codeSyntaxThemePreference.svelte";

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

  let savingFile = $state(false);
  let beginningEdit = $state(false);
  let handingOff = $state(false);
  const busy = $derived(savingFile || beginningEdit || handingOff);
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
  let lspSession = new CodeLspSession();
  const lspClient = $derived(lspSession.client);
  const lspError = $derived(lspSession.error);
  const lspConnecting = $derived(lspSession.connecting);
  const lspStatus = $derived(lspSession.status);
  const languageMatrix = $derived(lspSession.languageMatrix);
  const languageMatrixError = $derived(lspSession.languageMatrixError);
  let languageSessions = $state<CodeLanguageSessionSnapshot[]>([]);
  let languageSessionsLoading = $state(false);
  let languageSessionsError = $state<string | null>(null);
  let repairingLanguage = $state(false);
  let languageActionRunning = $state(false);
  let contextPanel = $state<"problems" | "outline" | "references" | "language" | null>(null);
  let problems = $state<ReturnType<CodeMirrorHost["getProblems"]>>([]);
  let workspaceProblems = $state<CodeProblem[]>([]);
  let workspaceProblemsScope = $state("");
  let workspaceProblemsLoaded = $state(false);
  let workspaceProblemsLoading = $state(false);
  let workspaceProblemsError = $state<string | null>(null);
  let workspaceProblemsUnavailableLanguages = $state<string[]>([]);
  let problemQuery = $state("");
  let problemSeverity = $state<CodeProblemSeverityFilter>("all");
  let problemsRequestEpoch = 0;
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
  let outputOpen = $state(false);
  let taskLiveStdout = $state("");
  let taskLiveStderr = $state("");
  let taskOutputTruncated = $state(false);
  let taskLiveLocations = $state<
    Array<{ path: string; line: number; column?: number | null; message: string }>
  >([]);
  let taskReadyUrl = $state<string | null>(null);
  let previewOpening = $state(false);
  let taskRunEventStream: CodeTaskRunEventStream | null = null;
  let projectTests = $state<ProjectTest[]>([]);
  let testsOpen = $state(false);
  let searchOpen = $state(false);
  let changesOpen = $state(false);
  let forgeChanges = $state<ForgeChanges | null>(null);
  let changesLoading = $state(false);
  let changesError = $state<string | null>(null);
  let changesRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  let selectedChangePath = $state<string | null>(null);
  let changeFileDiff = $state<ChangesFileDiff | null>(null);
  let changeFileLoading = $state(false);
  let changeFileError = $state<string | null>(null);
  let changeRestoreBusy = $state(false);
  let changeSyncBusy = $state(false);
  let changeSyncMessage = $state<string | null>(null);
  let changesHistory = $state<ChangesHistoryEntry[]>([]);
  let changesHistoryOpen = $state(false);
  let changesBlameOpen = $state(false);
  let changesBlameHunks = $state<ChangesBlameHunk[] | null>(null);
  let externalVersions = $state<Record<string, ForgeSourceFile>>({});
  let comparingTabId = $state<string | null>(null);
  let reviewChangedLines = $state<Array<{ line: number; kind: string }>>([]);
  let quickInput = $state<HTMLInputElement | null>(null);
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
  const problemSeverityOptions: Array<{
    value: CodeProblemSeverityFilter;
    label: string;
  }> = [
    { value: "all", label: "All" },
    { value: "error", label: "Errors" },
    { value: "warning", label: "Warnings" },
    { value: "information", label: "Info" },
  ];

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
    const needle = quickQuery.trim().replace(/^>/, "");
    return fuzzyMatchPaths(quickFiles, needle, 80);
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
  const workspaceProblemScopeKey = $derived(
    workId && workspaceRoot ? `${workId}\u0000${workspaceRoot}` : "",
  );
  const workspaceProblemLanguages = $derived(
    [...new Set(
      tabs
        .map((tab) => codeEditorLspLanguageId(tab.language))
        .filter((language) => languageSupportsLsp(language)),
    )].sort(),
  );
  const currentDocumentProblemFallback = $derived.by(() => {
    if (!activeTab || !documentUri || !workspaceRoot) return [];
    return normalizeCodeWorkspaceProblems(
      [{
        uri: documentUri,
        language: activeTab.language,
        diagnostics: problems.map((problem) => {
          const severity = problem.severity === "error"
            ? 1
            : problem.severity === "warning"
              ? 2
              : problem.severity === "info"
                ? 3
                : 4;
          return {
            message: problem.message,
            severity,
            range: {
              start: { line: Math.max(0, problem.line - 1), character: 0 },
              end: { line: Math.max(0, problem.line - 1), character: 0 },
            },
          };
        }),
      }],
      workspaceRoot,
    );
  });
  const effectiveWorkspaceProblems = $derived(
    workspaceProblemsLoaded && workspaceProblemsScope === workspaceProblemScopeKey
      ? workspaceProblems
      : currentDocumentProblemFallback,
  );
  const filteredWorkspaceProblems = $derived(
    filterCodeProblems(effectiveWorkspaceProblems, {
      query: problemQuery,
      severity: problemSeverity,
    }),
  );
  const workspaceProblemGroups = $derived(
    groupCodeProblems(filteredWorkspaceProblems),
  );
  const workspaceProblemCounts = $derived(
    countCodeProblems(effectiveWorkspaceProblems),
  );
  const latestLanguageSession = $derived(
    languageSessions.find((session) => session.kind === "editor") ?? languageSessions[0] ?? null,
  );
  const languageSessionLogs = $derived.by(() =>
    languageSessions
      .flatMap((session) =>
        session.logs.map((entry) => ({ ...entry, sessionId: session.id })),
      )
      .sort((a, b) => a.timestamp_ms - b.timestamp_ms || a.sequence - b.sequence)
      .slice(-500),
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

  function toggleOutput(forceOpen?: boolean) {
    outputOpen =
      forceOpen === true ? true : forceOpen === false ? false : !outputOpen;
  }

  function resetTaskOutputBuffers(run?: ProjectTaskRun | null) {
    taskLiveStdout = run?.stdout ?? "";
    taskLiveStderr = run?.stderr ?? "";
    taskOutputTruncated = run?.output_truncated ?? false;
    taskLiveLocations = run?.locations ?? [];
    taskReadyUrl = run?.ready_url ?? null;
  }

  function mergeTaskLocations(
    incoming:
      | Array<{ path: string; line: number; column?: number | null; message: string }>
      | null
      | undefined,
  ) {
    if (!incoming?.length) return;
    const next = [...taskLiveLocations];
    for (const location of incoming) {
      if (
        next.some(
          (existing) =>
            existing.path === location.path &&
            existing.line === location.line &&
            (existing.column ?? null) === (location.column ?? null),
        )
      ) {
        continue;
      }
      next.push(location);
      if (next.length >= 100) break;
    }
    taskLiveLocations = next;
  }

  function applyTaskRunEvent(event: ProjectTaskOutputEvent) {
    if (event.kind === "output" && event.text) {
      if (event.stream === "stderr") {
        taskLiveStderr += event.text;
      } else {
        taskLiveStdout += event.text;
      }
      mergeTaskLocations(event.locations);
      if (taskRun && event.run_id === taskRun.run_id) {
        taskRun = {
          ...taskRun,
          stdout: taskLiveStdout,
          stderr: taskLiveStderr,
          locations: taskLiveLocations,
          next_seq: event.seq + 1,
        };
      }
      return;
    }
    if (event.kind === "state") {
      mergeTaskLocations(event.locations);
      if (event.ready_url) taskReadyUrl = event.ready_url;
      if (taskRun && event.run_id === taskRun.run_id) {
        taskRun = {
          ...taskRun,
          state: event.state ?? taskRun.state,
          result: event.result ?? taskRun.result,
          stdout: event.result?.stdout ?? taskLiveStdout,
          stderr: event.result?.stderr ?? taskLiveStderr,
          output_truncated:
            event.result?.truncated ?? taskOutputTruncated,
          locations: event.result?.locations ?? taskLiveLocations,
          ready_url: event.ready_url ?? taskRun.ready_url ?? taskReadyUrl,
          next_seq: event.seq + 1,
        };
      }
      if (event.result) {
        taskLiveStdout = event.result.stdout;
        taskLiveStderr = event.result.stderr;
        taskOutputTruncated = event.result.truncated;
        taskLiveLocations = event.result.locations ?? taskLiveLocations;
        taskResult = event.result;
      } else if (event.state) {
        if (taskRun) taskRun = { ...taskRun, state: event.state };
      }
    }
  }

  function stopTaskRunEvents() {
    taskRunEventStream?.teardown();
    taskRunEventStream = null;
  }

  function startTaskRunEvents(workId: string, run: ProjectTaskRun) {
    stopTaskRunEvents();
    resetTaskOutputBuffers(run);
    const stream = new CodeTaskRunEventStream({
      onEvent: applyTaskRunEvent,
      onUnavailable: () => {
        /* polling fallback in runDetectedTask */
      },
      onTerminal: (result, state) => {
        if (result) {
          taskResult = result;
          taskLiveStdout = result.stdout;
          taskLiveStderr = result.stderr;
          taskOutputTruncated = result.truncated;
        }
        if (taskRun) {
          taskRun = {
            ...taskRun,
            state: state ?? taskRun.state,
            result: result ?? taskRun.result,
            stdout: result?.stdout ?? taskLiveStdout,
            stderr: result?.stderr ?? taskLiveStderr,
            output_truncated: result?.truncated ?? taskOutputTruncated,
          };
        }
      },
    });
    taskRunEventStream = stream;
    stream.start(workId, run.run_id, 0);
  }

  function taskRunStillActive(run: ProjectTaskRun): boolean {
    if (run.state === "running" || run.state === "ready") return true;
    // Cancel flips state before the process exits and final result lands.
    if (run.state === "cancelled" && !run.result) return true;
    return false;
  }

  async function runDetectedTask(test?: ProjectTest) {
    if (!selectedTask || runningTask) {
      onOpenTerminal?.();
      return;
    }
    runningTask = true;
    surfaceError = null;
    taskResult = null;
    outputOpen = true;
    try {
      let leaseId = context?.leaseId ?? null;
      let generation = context?.leaseGeneration ?? null;
      if ((!leaseId || generation == null) && canStartHumanEditing(detail?.allowed_actions)) {
        const begun = await startHumanEditingSession(detail!.id, detail!.allowed_actions);
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
      startTaskRunEvents(workId, taskRun);
      while (taskRunStillActive(taskRun)) {
        await new Promise((resolve) => setTimeout(resolve, 350));
        // Prefer live SSE buffers; poll snapshot as reconnect fallback.
        const snapshot = await getProjectTaskRun(workId, taskRun.run_id);
        taskRun = {
          ...snapshot,
          stdout: snapshot.stdout || taskLiveStdout || snapshot.stdout,
          stderr: snapshot.stderr || taskLiveStderr || snapshot.stderr,
        };
        if (snapshot.stdout) taskLiveStdout = snapshot.stdout;
        if (snapshot.stderr) taskLiveStderr = snapshot.stderr;
        taskOutputTruncated = snapshot.output_truncated ?? taskOutputTruncated;
        if (snapshot.locations?.length) mergeTaskLocations(snapshot.locations);
        if (snapshot.ready_url) taskReadyUrl = snapshot.ready_url;
        if (snapshot.result) {
          taskResult = snapshot.result;
          taskLiveStdout = snapshot.result.stdout;
          taskLiveStderr = snapshot.result.stderr;
          taskOutputTruncated = snapshot.result.truncated;
          taskLiveLocations = snapshot.result.locations ?? taskLiveLocations;
        }
      }
      taskResult = taskRun.result ?? taskResult;
      await undertakings.refreshDetail();
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      stopTaskRunEvents();
      runningTask = false;
    }
  }

  async function openTaskPreview() {
    if (!(taskReadyUrl || taskRun?.ready_url) || previewOpening) return;
    previewOpening = true;
    surfaceError = null;
    try {
      if (!taskRun) throw new Error("No task run is available");
      const { url } = await resolveTaskPreviewOpenUrl(workId, {
        ...taskRun,
        ready_url: taskReadyUrl ?? taskRun.ready_url,
      });
      await openInBrowser(url, {
        openedBy: "user",
        workCardId: workId,
        title: taskRun.task.label,
      });
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      previewOpening = false;
    }
  }

  async function stopDetectedTask() {
    if (!taskRun || (taskRun.state !== "running" && taskRun.state !== "ready")) return;
    try {
      taskRun = await cancelProjectTaskRun(workId, taskRun.run_id);
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    }
  }

  async function toggleTests() {
    const next = !testsOpen;
    testsOpen = next;
    if (workId) {
      codeWorkbenchState.setTestsOpen(workId, next);
      codeWorkspace.scheduleLayoutPersist(workId);
    }
    if (!next || projectTests.length || !workId) return;
    try {
      projectTests = await getProjectTests(workId);
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    }
  }

  function toggleSearch(forceOpen?: boolean) {
    const next =
      forceOpen === true ? true : forceOpen === false ? false : !searchOpen;
    searchOpen = next;
    if (!workId) return;
    codeWorkbenchState.setSearchOpen(workId, next);
    codeWorkspace.scheduleLayoutPersist(workId);
  }

  async function refreshForgeChanges() {
    if (!workId || !changesOpen) return;
    changesLoading = true;
    changesError = null;
    try {
      forgeChanges = await getForgeChanges(workId);
      if (
        selectedChangePath &&
        !forgeChanges.files.some((file) => file.path === selectedChangePath)
      ) {
        selectedChangePath = null;
        changeFileDiff = null;
        changeFileError = null;
      } else if (selectedChangePath) {
        await loadChangeFileDiff(selectedChangePath);
      }
    } catch (err) {
      if (isMissingForgeRoute(err)) {
        changesError = "This workshop does not expose Changes yet — update the daemon.";
      } else {
        changesError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      changesLoading = false;
    }
  }

  function scheduleChangesRefresh() {
    if (!changesOpen || !workId) return;
    if (changesRefreshTimer) clearTimeout(changesRefreshTimer);
    changesRefreshTimer = setTimeout(() => {
      changesRefreshTimer = null;
      void refreshForgeChanges();
    }, 200);
  }

  async function loadChangeFileDiff(path: string) {
    if (!workId) return;
    changeFileLoading = true;
    changeFileError = null;
    try {
      changeFileDiff = await getChangesFile(workId, path);
    } catch (err) {
      changeFileDiff = null;
      if (isMissingForgeRoute(err)) {
        changeFileError = "This workshop does not expose Changes diffs yet — update the daemon.";
      } else {
        changeFileError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      changeFileLoading = false;
    }
  }

  async function selectChangePath(path: string) {
    selectedChangePath = path;
    changesBlameOpen = false;
    changesBlameHunks = null;
    await loadChangeFileDiff(path);
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

  async function restoreChangeFile(diff: ChangesFileDiff) {
    if (!workId) return;
    changeRestoreBusy = true;
    try {
      const lease = await ensureHumanLease();
      await restoreChangesFile(workId, {
        path: diff.path,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      await refreshForgeChanges();
      reconcileOpenFiles();
      try {
        quickFiles = (await getUndertakingSourceTree(workId)).files;
      } catch {
        /* tree refresh can retry later */
      }
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      changeRestoreBusy = false;
    }
  }

  async function revertChangeHunk(diff: ChangesFileDiff, hunkIndex: number) {
    if (!workId || !diff.working_digest) return;
    changeRestoreBusy = true;
    try {
      const lease = await ensureHumanLease();
      await revertChangesHunk(workId, {
        path: diff.path,
        hunk_index: hunkIndex,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      await refreshForgeChanges();
      reconcileOpenFiles();
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      changeRestoreBusy = false;
    }
  }

  async function resolveChangeConflict(
    diff: ChangesFileDiff,
    resolution: "ours" | "theirs" | "baseline",
  ) {
    if (!workId) return;
    changeRestoreBusy = true;
    try {
      const lease = await ensureHumanLease();
      const result = await resolveChangesConflict(workId, {
        path: diff.path,
        resolution,
        expected_working_digest: diff.working_digest,
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      forgeChanges = result.changes;
      await loadChangeFileDiff(diff.path);
      reconcileOpenFiles();
      changeSyncMessage = `Conflict resolved (${resolution})`;
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      changeRestoreBusy = false;
    }
  }

  async function runChangesSync(
    action: "fetch" | "pull" | "push" | "sync",
  ) {
    if (!workId) return;
    changeSyncBusy = true;
    changeSyncMessage = null;
    try {
      const lease = await ensureHumanLease();
      const body = {
        lease_id: lease.leaseId,
        generation: lease.generation,
      };
      const result =
        action === "fetch"
          ? await fetchChanges(workId, body)
          : action === "pull"
            ? await pullChanges(workId, body)
            : action === "push"
              ? await pushChanges(workId, body)
              : await syncChanges(workId, body);
      forgeChanges = result.changes;
      changeSyncMessage = result.message;
      if (selectedChangePath) await loadChangeFileDiff(selectedChangePath);
      reconcileOpenFiles();
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      changeSyncBusy = false;
    }
  }

  async function sealChangesForReview() {
    if (!workId) return;
    changeSyncBusy = true;
    try {
      const lease = await ensureHumanLease();
      await checkpointChanges(workId, {
        lease_id: lease.leaseId,
        generation: lease.generation,
      });
      await undertakings.refreshDetail();
      await lmeWorkspace.openCodeReview(workId, `Review · ${detail?.title ?? "project"}`);
      changeSyncMessage = "Sealed for Review";
      await refreshForgeChanges();
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    } finally {
      changeSyncBusy = false;
    }
  }

  async function toggleChangesHistory() {
    changesHistoryOpen = !changesHistoryOpen;
    if (!changesHistoryOpen || !workId) return;
    try {
      const result = await getChangesHistory(workId, 40);
      changesHistory = result.commits;
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    }
  }

  async function toggleChangesBlame() {
    changesBlameOpen = !changesBlameOpen;
    if (!changesBlameOpen) {
      changesBlameHunks = null;
      return;
    }
    if (!workId || !selectedChangePath) return;
    changesBlameHunks = null;
    try {
      const result = await getChangesBlame(workId, selectedChangePath);
      changesBlameHunks = result.hunks;
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
      changesBlameOpen = false;
    }
  }

  async function toggleChanges(forceOpen?: boolean) {
    const next =
      forceOpen === true ? true : forceOpen === false ? false : !changesOpen;
    changesOpen = next;
    if (workId) {
      codeWorkbenchState.setChangesOpen(workId, next);
      codeWorkspace.scheduleLayoutPersist(workId);
    }
    if (next) await refreshForgeChanges();
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
      diagnostics: effectiveWorkspaceProblems.slice(0, 20).map((problem) =>
        `${problem.path}:${problem.line}:${problem.character} ${problem.message}`
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
      if (workId) {
        codeWorkbenchState.setTerminalOpen(workId, false);
        codeWorkspace.scheduleLayoutPersist(workId);
      }
      return;
    }
    if (!detail || !terminalAvailable) {
      surfaceError = "Terminal is not available for this project yet.";
      return;
    }
    terminalDockOpen = true;
    if (workId) {
      codeWorkbenchState.setTerminalOpen(workId, true);
      codeWorkspace.scheduleLayoutPersist(workId);
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
      terminalDockOpen = false;
      if (workId) {
        codeWorkbenchState.setTerminalOpen(workId, false);
        codeWorkspace.scheduleLayoutPersist(workId);
      }
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
    terminalDockOpen = false;
    if (workId) {
      codeWorkbenchState.setTerminalOpen(workId, false);
      codeWorkspace.scheduleLayoutPersist(workId);
    }
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
    if (resolved === "plaintext") {
      return codeEditorLspLanguageId(activeTabLanguage);
    }
    return codeEditorLspLanguageId(resolved);
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

  async function refreshLanguageSessions(options?: { quiet?: boolean }) {
    if (!workId || !documentUri || !languageSupportsLsp(activeTabLanguage)) {
      languageSessions = [];
      languageSessionsError = null;
      return;
    }
    if (!options?.quiet) languageSessionsLoading = true;
    try {
      const snapshot = await getCodeLanguageSessions({
        workId,
        uri: documentUri,
        language: activeLspLanguage,
      });
      languageSessions = snapshot.sessions;
      languageSessionsError = null;
    } catch (err) {
      languageSessionsError = err instanceof Error ? err.message : String(err);
    } finally {
      languageSessionsLoading = false;
    }
  }

  async function showLanguageLogs() {
    await showLanguagePanel();
  }

  function formatLanguageLogTime(timestamp: number): string {
    return new Date(timestamp).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
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
    const id = workId;
    treeRefreshTimer = setTimeout(() => {
      treeRefreshTimer = null;
      void getUndertakingSourceTree(id)
        .then((tree) => {
          if (workId === id) quickFiles = tree.files;
        })
        .catch(() => {
          /* quiet */
        });
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
    scheduleChangesRefresh();
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

  async function startEditing() {
    if (!detail || !canStartHumanEditing(detail.allowed_actions)) return;
    if (beginEditPromise) {
      await beginEditPromise;
      return;
    }
    beginningEdit = true;
    surfaceError = null;
    beginEditPromise = (async () => {
      const begun = await startHumanEditingSession(detail.id, detail.allowed_actions);
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
      surfaceError = humanizeForgeMessage(
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      beginEditPromise = null;
      beginningEdit = false;
    }
  }

  async function onDraftChanged(tabIdValue: string, value: string) {
    codeWorkspace.updateDraft(tabIdValue, value);
    if (!editable && canBeginEdit) {
      await startEditing();
    }
  }

  async function saveTab(tab: CodeDocumentTab | null): Promise<boolean> {
    if (!tab) return true;
    if (tab.preview) {
      surfaceError = CODE_SAVE_PREVIEW_ERROR;
      return false;
    }
    if (tab.tabId === activeTabId && editor) {
      const liveDraft = editor.getValue();
      if (liveDraft !== tab.draft) {
        codeWorkspace.updateDraft(tab.tabId, liveDraft);
        tab = { ...tab, draft: liveDraft };
      }
      editor.flushChanges();
    }

    const decision = decideCodeSave({
      preview: Boolean(tab.preview),
      dirty: codeWorkspace.isDirty(tab),
      savingFile,
      hasLease: Boolean(
        context?.workId === workId &&
          context.leaseId &&
          context.leaseGeneration != null,
      ),
      canBeginEdit,
      beginningEdit: beginningEdit || Boolean(beginEditPromise),
    });

    if (decision.action === "noop") {
      return decision.reason === "not-dirty" || decision.reason === "already-saving";
    }
    if (decision.action === "reject") {
      surfaceError =
        decision.reason === "preview"
          ? CODE_SAVE_PREVIEW_ERROR
          : CODE_SAVE_NO_LEASE_ERROR;
      return false;
    }
    if (decision.action === "await-lease") {
      if (beginEditPromise) {
        try {
          await beginEditPromise;
        } catch (err) {
          surfaceError = humanizeForgeMessage(
            err instanceof Error ? err.message : String(err),
          );
          return false;
        }
      }
    } else if (decision.action === "begin-then-save") {
      try {
        await startEditing();
      } catch (err) {
        surfaceError = humanizeForgeMessage(
          err instanceof Error ? err.message : String(err),
        );
        return false;
      }
    }

    let leaseId = context?.leaseId ?? null;
    let generation = context?.leaseGeneration ?? null;
    if (!leaseId || generation == null) {
      try {
        const lease = await ensureHumanLease();
        leaseId = lease.leaseId;
        generation = lease.generation;
      } catch (err) {
        surfaceError = humanizeForgeMessage(
          err instanceof Error ? err.message : String(err),
        );
        return false;
      }
    }
    if (!leaseId || generation == null || !codeWorkspace.isDirty(tab) || savingFile) {
      return !codeWorkspace.isDirty(tab);
    }

    savingFile = true;
    saveWhisper = "Saving…";
    if (saveWhisperTimer) clearTimeout(saveWhisperTimer);
    surfaceError = null;
    codeWorkspace.setError(tab.tabId, null);
    try {
      const next = await saveUndertakingSource(workId, {
        path: tab.path,
        content: tab.draft,
        lease_id: leaseId,
        generation,
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
      const message = humanizeForgeMessage(
        err instanceof Error ? err.message : String(err),
      );
      codeWorkspace.setError(tab.tabId, message);
      surfaceError = message;
      return false;
    } finally {
      savingFile = false;
    }
  }

  async function save() {
    const ok = await saveTab(activeTab);
    if (!ok && !surfaceError && activeTab && codeWorkspace.isDirty(activeTab)) {
      surfaceError = "Could not save the file.";
    }
  }

  async function saveAll(): Promise<boolean> {
    for (const tab of tabs) {
      if (codeWorkspace.isDirty(tab) && !(await saveTab(tab))) return false;
    }
    return true;
  }

  async function handoffToAgent(draft?: string) {
    if (!onHandoffToAgent || busy) return;
    surfaceError = null;
    if (!(await saveAll())) {
      surfaceError = "Resolve the unsaved file before asking an agent to continue.";
      return;
    }
    handingOff = true;
    try {
      captureEditorContext();
      await onHandoffToAgent(preferredAgent, draft);
    } catch (err) {
      surfaceError = humanizeForgeMessage(
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      handingOff = false;
    }
  }

  async function reclaimHuman() {
    if (!onReclaimHuman || busy) return;
    handingOff = true;
    surfaceError = null;
    try {
      await onReclaimHuman();
    } catch (err) {
      surfaceError = humanizeForgeMessage(
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      handingOff = false;
    }
  }

  function syncProblems() {
    void tick().then(() => {
      problems = editor?.getProblems() ?? [];
    });
  }

  async function refreshWorkspaceProblems(options?: { quiet?: boolean }) {
    const requestWorkId = workId;
    const requestRoot = workspaceRoot;
    const requestScope = requestWorkId && requestRoot
      ? `${requestWorkId}\u0000${requestRoot}`
      : "";
    const requestLanguages = [...workspaceProblemLanguages];
    const requestEpoch = ++problemsRequestEpoch;
    if (!requestScope || !requestRoot) {
      workspaceProblems = [];
      workspaceProblemsScope = "";
      workspaceProblemsLoaded = false;
      workspaceProblemsLoading = false;
      workspaceProblemsError = null;
      workspaceProblemsUnavailableLanguages = [];
      return;
    }
    if (workspaceProblemsScope !== requestScope) {
      workspaceProblems = [];
      workspaceProblemsScope = requestScope;
      workspaceProblemsLoaded = false;
      workspaceProblemsUnavailableLanguages = [];
    }
    if (!options?.quiet || !workspaceProblemsLoaded) workspaceProblemsLoading = true;
    workspaceProblemsError = null;
    try {
      const snapshot = await getAllCodeWorkspaceDiagnostics({
        workId: requestWorkId,
        languages: requestLanguages,
      });
      if (requestEpoch !== problemsRequestEpoch || workspaceProblemScopeKey !== requestScope) {
        return;
      }
      workspaceProblems = normalizeCodeWorkspaceProblems(snapshot.documents, requestRoot);
      workspaceProblemsUnavailableLanguages = snapshot.unavailableLanguages ?? [];
      workspaceProblemsLoaded = true;
    } catch (err) {
      if (requestEpoch !== problemsRequestEpoch || workspaceProblemScopeKey !== requestScope) {
        return;
      }
      workspaceProblemsError = err instanceof Error ? err.message : String(err);
    } finally {
      if (requestEpoch === problemsRequestEpoch) workspaceProblemsLoading = false;
    }
  }

  async function openWorkspaceProblem(problem: CodeProblem) {
    surfaceError = null;
    try {
      const tab = await lmeWorkspace.openCodeFile(workId, problem.path, {
        line: problem.line,
      });
      undertakings.setSelection({
        path: problem.path,
        line: problem.line,
        entityId: null,
      });
      await tick();
      if (tab) editor?.revealLine(problem.line);
    } catch (err) {
      surfaceError = err instanceof Error ? err.message : String(err);
    }
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

  function setContextPanel(next: CodeContextPanel) {
    contextPanel = next;
    if (!workId) return;
    codeWorkbenchState.setContextPanel(workId, next);
    codeWorkspace.scheduleLayoutPersist(workId);
  }

  async function showOutline() {
    const next = contextPanel === "outline" ? null : "outline";
    setContextPanel(next);
    if (next === "outline") await refreshSymbols();
  }

  async function showLanguagePanel() {
    const next = contextPanel === "language" ? null : "language";
    setContextPanel(next);
    if (next === "language") await refreshLanguageSessions();
  }

  async function showProblems() {
    const next = contextPanel === "problems" ? null : "problems";
    setContextPanel(next);
    if (next !== "problems") return;
    lspClient?.sync();
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
      quickFiles = (await getUndertakingSourceTree(workId)).files;
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
      saveWhisper = "Refactor applied";
      if (saveWhisperTimer) clearTimeout(saveWhisperTimer);
      saveWhisperTimer = setTimeout(() => (saveWhisper = null), 1800);
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
        setContextPanel("references");
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
      diagnostics: effectiveWorkspaceProblems.slice(0, 20).map((problem) =>
        `${problem.path}:${problem.line}:${problem.character} ${problem.message}`
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
    if (lspSession.scope !== scope) {
      languageSessions = [];
      languageSessionsError = null;
    }
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
    const scope = workspaceProblemScopeKey;
    const languagesKey = workspaceProblemLanguages.join("\u0000");
    const showingProblems = contextPanel === "problems";
    void languagesKey;
    void lspClient;
    if (!interactive || !scope) {
      untrack(() => {
        if (workspaceProblemsScope) void refreshWorkspaceProblems({ quiet: true });
      });
      return;
    }
    const refreshTimer = setTimeout(
      () => untrack(() => void refreshWorkspaceProblems({ quiet: !showingProblems })),
      showingProblems ? 0 : 350,
    );
    if (!showingProblems) return () => clearTimeout(refreshTimer);
    const pollingTimer = setInterval(
      () => untrack(() => void refreshWorkspaceProblems({ quiet: true })),
      2_000,
    );
    return () => {
      clearTimeout(refreshTimer);
      clearInterval(pollingTimer);
    };
  });

  $effect(() => {
    const showingLanguage = contextPanel === "language";
    const scope = `${workId}:${activeTabLanguage}:${documentUri ?? ""}`;
    void scope;
    if (!showingLanguage || !workId || !documentUri) return;
    // Permanent path/policy failures will not clear by polling — stop the spam.
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
    problems = [];
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
      issueCount: workspaceProblemCounts.total,
      dirty,
      saving: busy,
      saveWhisper,
      control: statusControlLabel,
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
    const onShowProblems = () => void showProblems();
    const onCodeCommand = (event: Event) => {
      const id = (event as CustomEvent<{ id?: string }>).detail?.id;
      if (!id) return;
      switch (id) {
        case "workbench.action.quickOpen":
          void showQuickOpen();
          break;
        case "workbench.action.navigateBack":
          void navigate(-1);
          break;
        case "workbench.action.navigateForward":
          void navigate(1);
          break;
        case "workbench.actions.view.problems":
          void showProblems();
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
          void toggleTests();
          break;
        case "workbench.action.findInFiles":
          toggleSearch(true);
          break;
        case "workbench.view.scm":
          void toggleChanges(true);
          break;
        case "git.fetch":
          void toggleChanges(true).then(() => runChangesSync("fetch"));
          break;
        case "git.pull":
          void toggleChanges(true).then(() => runChangesSync("pull"));
          break;
        case "git.push":
          void toggleChanges(true).then(() => runChangesSync("push"));
          break;
        case "git.sync":
          void toggleChanges(true).then(() => runChangesSync("sync"));
          break;
        case "medousa.forge.checkpoint":
          void toggleChanges(true).then(() => sealChangesForReview());
          break;
        case "git.viewHistory":
          void toggleChanges(true).then(() => {
            if (!changesHistoryOpen) void toggleChangesHistory();
          });
          break;
        case "git.blame.toggle":
          void toggleChanges(true).then(() => toggleChangesBlame());
          break;
        case "workbench.action.output.toggleOutput":
          toggleOutput();
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
      contextPanel = layout.context_panel;
      testsOpen = layout.tests;
      searchOpen = layout.search;
      changesOpen = layout.changes;
      if (layout.terminal) void toggleTerminalDock(true);
      else terminalDockOpen = false;
      if (layout.changes) void refreshForgeChanges();
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
    if (changesRefreshTimer) clearTimeout(changesRefreshTimer);
    projectEventStream?.teardown();
    projectEventStream = null;
    stopTaskRunEvents();
    codeEditorStatus.clear(statusOwnerId);
  });
</script>

<section class="code-source-editor flex flex-col overflow-hidden {fill ? 'code-source-editor--fill min-h-0 flex-1' : 'min-h-[26rem] rounded-lg border border-surface-500/35 bg-surface-950/45'}">
  <header class="code-editor-chrome relative z-20 flex shrink-0 items-center gap-1.5 border-b border-surface-500/30 px-1.5">
    <div class="flex min-w-0 flex-1 items-center gap-1">
      <div class="flex shrink-0 items-center">
        <button type="button" class="scripts-workbench-toolbar-btn" aria-label="Go back" title="Go back" disabled={!activeTab || !codeWorkspace.canNavigate(workId, -1)} onclick={() => void navigate(-1)}><ArrowLeft size={14} strokeWidth={1.75} /></button>
        <button type="button" class="scripts-workbench-toolbar-btn" aria-label="Go forward" title="Go forward" disabled={!activeTab || !codeWorkspace.canNavigate(workId, 1)} onclick={() => void navigate(1)}><ArrowRight size={14} strokeWidth={1.75} /></button>
      </div>
      {#if projectTitle}
        <span class="code-editor-chrome-title" title={projectTitle}>{projectTitle}</span>
        {#if activeTab}
          <span class="code-editor-chrome-sep" aria-hidden="true">›</span>
        {:else if phaseLabel}
          <span class="code-editor-chrome-phase">{phaseLabel}</span>
        {/if}
      {/if}
      {#if activeTab}
        <CodeBreadcrumbs
          path={activeTab.path}
          onPathSegment={onBreadcrumbPath}
        />
      {/if}
    </div>
    <div class="code-editor-chrome-actions flex shrink-0 items-center gap-0.5">
      {#if reviewAvailable}
        <button
          type="button"
          class="scripts-workbench-toolbar-btn text-amber-300/85"
          title="Review changes"
          aria-label="Review changes"
          onclick={onOpenReview}
        >
          <GitPullRequestArrow size={14} strokeWidth={1.75} />
        </button>
      {/if}
      {#if agentRunning}
        <button
          type="button"
          class="scripts-workbench-toolbar-btn flex items-center gap-1 text-amber-300"
          title={`Stop ${agentLabel}`}
          aria-label={`Stop ${agentLabel}`}
          onclick={onStopAgent}
        ><Square size={13} strokeWidth={1.75} /></button>
        <button
          type="button"
          class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
          title="Resume editing"
          aria-label="Resume editing"
          onclick={() => {
            if (onResumeEditing) onResumeEditing();
            else void reclaimHuman();
          }}
        ><Play size={14} strokeWidth={1.75} /></button>
      {/if}
      {#if activeTab}
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
          class="scripts-workbench-toolbar-btn {searchOpen ? 'scripts-workbench-toolbar-btn-active' : ''}"
          title={titleWithShortcut("Search in files", "code-search")}
          aria-label="Search in files"
          aria-pressed={searchOpen}
          onclick={() => toggleSearch()}
        ><Search size={14} strokeWidth={1.75} /></button>
        <button
          type="button"
          class="scripts-workbench-toolbar-btn {changesOpen ? 'scripts-workbench-toolbar-btn-active' : ''}"
          title="Changes"
          aria-label="Show changes"
          aria-pressed={changesOpen}
          onclick={() => void toggleChanges()}
        ><GitBranch size={14} strokeWidth={1.75} /></button>
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
        {#if runningTask}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-active"
            title={selectedTask ? `Stop ${selectedTask.label}` : "Stop task"}
            aria-label={selectedTask ? `Stop ${selectedTask.label}` : "Stop task"}
            onclick={() => void stopDetectedTask()}
          ><X size={14} strokeWidth={1.75} /></button>
        {/if}

        <span class="code-editor-chrome-divider" aria-hidden="true"></span>

        {#if agentHasControl && !agentRunning}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={busy}
            onclick={() => void reclaimHuman()}
            aria-label="Resume editing"
            title="Resume editing — take the file back from the agent"
          ><UserRound size={14} strokeWidth={1.75} /></button>
        {:else if !editable && canBeginEdit}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={busy}
            onclick={() => void startEditing()}
            aria-label="Edit file"
            title="Start editing (or just type)"
          ><Pencil size={14} strokeWidth={1.75} /></button>
        {:else}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={!editable || !dirty || savingFile}
            onclick={() => void save()}
            aria-label="Save file"
            title={savingFile ? "Saving…" : titleWithShortcut("Save", "code-save")}
          ><Save size={14} strokeWidth={1.75} /></button>
        {/if}

        <OverflowMenu
          label="Editor options"
          title="Editor options"
          panelClass="w-[min(16rem,calc(100vw-2rem))] rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
        >
          {#snippet trigger({ open, toggle })}
            <button
              type="button"
              class="scripts-workbench-toolbar-btn {open ? 'scripts-workbench-toolbar-btn-active' : ''}"
              title="Editor options — wrap, font, structure, language server"
              aria-label="Editor options"
              aria-expanded={open}
              aria-haspopup="menu"
              onclick={toggle}
            >
              <Settings2 size={14} strokeWidth={1.75} />
            </button>
          {/snippet}
          <button type="button" role="menuitem" class="code-chrome-menu-item" title={titleWithShortcut("Find in file", "code-find")} onclick={() => editor?.openFind()}>Find in file</button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={() => toggleOutput()}>
            <span>Output</span>
            <span class="code-chrome-menu-meta">{outputOpen ? "On" : "Off"}</span>
          </button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" disabled={!lspClient} onclick={() => void showOutline()}>
            <span>{titleWithShortcut("Structure", "code-structure")}</span>
          </button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={onToggleWorld}>
            <span>Understand this code</span>
            <span class="code-chrome-menu-meta">{worldOpen ? "On" : "Off"}</span>
          </button>
          {#if selectedTask && !runningTask}
            <button
              type="button"
              role="menuitem"
              class="code-chrome-menu-item"
              title={`${selectedTask.label}: ${selectedTask.argv.join(" ")}`}
              onclick={() => void runDetectedTask()}
            >Run {selectedTask.label}</button>
          {/if}
          <button type="button" role="menuitem" class="code-chrome-menu-item" disabled={activeTab.loading || busy} onclick={() => void reload()}>Reload file</button>
          <div class="code-chrome-menu-sep" role="separator"></div>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={toggleWordWrap}>
            <span>Word wrap</span>
            <span class="code-chrome-menu-meta">{wordWrap ? "On" : "Off"}</span>
          </button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={toggleLineNumbers}>
            <span>Line numbers</span>
            <span class="code-chrome-menu-meta">{showLineNumbers ? "On" : "Off"}</span>
          </button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={cycleFontSize}>
            <span>Font size</span>
            <span class="code-chrome-menu-meta">{fontSize}px</span>
          </button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={openSyntaxThemeSettings}>
            <span>Syntax theme</span>
            <span class="code-chrome-menu-meta">{codeSyntaxThemePreference.theme.label}</span>
          </button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={cycleTabSize}>
            <span>Tab size</span>
            <span class="code-chrome-menu-meta">{tabSizePref}</span>
          </button>
          {#if projectTasks.length > 1}
            <label class="code-chrome-menu-field">
              <span class="code-chrome-menu-field-label">Project command</span>
              <select class="code-chrome-menu-select" aria-label="Project command" bind:value={selectedTaskId}>
                {#each projectTasks as task (task.id)}
                  <option value={task.id}>{task.label}{#if task.long_running} · background{/if}{#if task.provider === "vscode-tasks"} · tasks.json{/if}</option>
                {/each}
              </select>
            </label>
          {/if}
          {#if projectTasks.some((task) => task.kind === "test")}
            <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={() => void toggleTests()}>
              <span>Discovered tests</span>
              <span class="code-chrome-menu-meta">{testsOpen ? "Hide" : "Show"}</span>
            </button>
          {/if}
          {#if canFormat && editable}
            <button type="button" role="menuitem" class="code-chrome-menu-item" disabled={languageActionRunning} onclick={() => void runLanguageAction("format")}>Format document</button>
          {/if}
          {#if canCodeAction && editable}
            <button type="button" role="menuitem" class="code-chrome-menu-item" disabled={languageActionRunning} onclick={() => void runLanguageAction("organize_imports")}>Organize imports</button>
          {/if}
          {#if languageSupportsLsp(activeTabLanguage)}
            <div class="code-chrome-menu-sep" role="separator"></div>
            <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={restartLanguageServer}>Restart language server</button>
            <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={() => void showLanguageLogs()}>Show language server logs</button>
          {/if}
          {#if lspError}
            <button type="button" role="menuitem" class="code-chrome-menu-item code-chrome-menu-item--warn" disabled={repairingLanguage} onclick={() => void repairLanguageSupport()}>{repairingLanguage ? "Repairing…" : "Repair language support"}</button>
          {/if}
        </OverflowMenu>
        {#if projectMenu}
          <OverflowMenu
            label="Project actions"
            title="Project actions"
            panelClass="w-52 rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
          >
            {#snippet trigger({ open, toggle })}
              <button
                type="button"
                class="scripts-workbench-toolbar-btn {open ? 'scripts-workbench-toolbar-btn-active' : ''}"
                title="Project actions — agent, terminal, discard"
                aria-label="Project actions"
                aria-expanded={open}
                aria-haspopup="menu"
                onclick={toggle}
              >
                <FolderKanban size={14} strokeWidth={1.75} />
              </button>
            {/snippet}
            {@render projectMenu()}
          </OverflowMenu>
        {/if}
      {/if}
    </div>
  </header>

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
        <button type="button" class="rounded px-1.5 py-0.5 hover:bg-white/10" onclick={() => useProjectVersion(activeTab)}>Use project version</button>
        <button type="button" class="rounded bg-amber-500/20 px-1.5 py-0.5" onclick={() => keepDraft(activeTab)}>Keep my draft</button>
      </div>
    {/if}

    <div class="flex min-h-0 flex-1 overflow-hidden">
      <div class="relative min-h-0 min-w-0 flex-1">
        {#if editorSelection?.text && onHandoffToAgent && !agentHasControl}
          <div class="absolute right-3 top-2 z-20 flex max-w-[calc(100%-1.5rem)] items-center gap-1 overflow-x-auto rounded-md border border-primary-500/30 bg-surface-950/95 px-1.5 py-1 shadow-xl" aria-label="Selected code actions">
            <span class="mr-1 flex shrink-0 items-center gap-1 text-chrome-xs text-primary-200/80"><Sparkles size={10} />Selection</span>
            <button type="button" class="code-intent-action" disabled={busy} onclick={() => void handoffToAgent("Help me understand the selected code and answer my questions about it.")}>Ask</button>
            <button type="button" class="code-intent-action" disabled={busy} onclick={() => void handoffToAgent("Change the selected code. Ask only if the intended change is ambiguous.")}>Change</button>
            {#if problems.length > 0}<button type="button" class="code-intent-action" disabled={busy} onclick={() => void handoffToAgent("Fix the relevant issue in the selected code and verify the result.")}>Fix</button>{/if}
            <button type="button" class="code-intent-action" disabled={busy} onclick={() => void handoffToAgent("Explain the selected code clearly, including its role and important behavior.")}>Explain</button>
            <button type="button" class="code-intent-action" disabled={busy} onclick={() => void handoffToAgent("Add the most valuable focused test for the selected code and run the relevant check.")}>Add test</button>
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
            This file could not be opened.
          </div>
        {/if}
      </div>
    </div>

    {#if contextPanel}
      <div class="{contextPanel === 'problems' || contextPanel === 'language' ? 'max-h-72' : 'max-h-44'} shrink-0 overflow-y-auto border-t border-surface-500/30 bg-surface-950/80">
        <div class="sticky top-0 z-10 flex items-center justify-between border-b border-surface-500/25 bg-surface-950 px-2 py-1">
          <div class="flex min-w-0 items-center gap-2">
            <span class="text-chrome-xs font-medium uppercase tracking-wider text-content-tertiary">
              {contextPanel === "problems" ? "Problems" : contextPanel === "references" ? "Uses" : contextPanel === "language" ? "Language server" : "Structure"}
            </span>
            {#if contextPanel === "problems"}
              <span class="text-chrome-xs text-rose-300" title="Errors">{workspaceProblemCounts.errors}</span>
              <span class="text-chrome-xs text-amber-300" title="Warnings">{workspaceProblemCounts.warnings}</span>
              <span class="text-chrome-xs text-content-quiet" title="Information and hints">{workspaceProblemCounts.information + workspaceProblemCounts.hints}</span>
            {/if}
          </div>
          <div class="flex items-center gap-0.5">
            {#if contextPanel === "problems"}
              <button
                type="button"
                class="rounded p-0.5 text-content-quiet hover:bg-surface-800 hover:text-surface-200 disabled:opacity-50"
                aria-label="Refresh project problems"
                title="Refresh project problems"
                disabled={workspaceProblemsLoading}
                onclick={() => void refreshWorkspaceProblems()}
              ><RotateCcw size={11} class={workspaceProblemsLoading ? "animate-spin" : ""} /></button>
            {:else if contextPanel === "language"}
              <button
                type="button"
                class="rounded p-0.5 text-content-quiet hover:bg-surface-800 hover:text-surface-200 disabled:opacity-50"
                aria-label="Refresh language server logs"
                title="Refresh language server logs"
                disabled={languageSessionsLoading}
                onclick={() => void refreshLanguageSessions()}
              ><RotateCcw size={11} class={languageSessionsLoading ? "animate-spin" : ""} /></button>
            {/if}
            <button type="button" class="rounded p-0.5 text-content-quiet hover:text-surface-200" aria-label="Close context panel" onclick={() => setContextPanel(null)}><X size={11} /></button>
          </div>
        </div>
        {#if contextPanel === "problems"}
          <div class="flex items-center gap-1.5 border-b border-surface-500/20 px-2 py-1.5">
            <label class="flex min-w-28 flex-1 items-center gap-1.5 rounded border border-surface-500/35 bg-surface-900/70 px-1.5 py-1 focus-within:border-primary-400/60">
              <Search size={11} class="shrink-0 text-content-quiet" />
              <input
                class="min-w-0 flex-1 bg-transparent text-chrome-sm text-content-secondary outline-none placeholder:text-content-faint"
                aria-label="Filter project problems"
                placeholder="Filter problems"
                bind:value={problemQuery}
              />
            </label>
            <div class="flex items-center gap-0.5" aria-label="Problem severity filter">
              {#each problemSeverityOptions as option (option.value)}
                <button
                  type="button"
                  class="rounded px-1.5 py-1 text-chrome-xs {problemSeverity === option.value ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800 hover:text-content-secondary'}"
                  aria-pressed={problemSeverity === option.value}
                  onclick={() => (problemSeverity = option.value)}
                >{option.label}</button>
              {/each}
            </div>
          </div>
          {#if workspaceProblemsError}
            <div class="flex items-start justify-between gap-3 border-b border-rose-400/20 bg-rose-500/5 px-3 py-2 text-chrome-sm text-rose-200">
              <span>Could not refresh project problems: {workspaceProblemsError}</span>
              <button type="button" class="shrink-0 underline underline-offset-2" onclick={() => void refreshWorkspaceProblems()}>Retry</button>
            </div>
          {/if}
          {#if workspaceProblemsUnavailableLanguages.length > 0}
            <p class="border-b border-amber-400/20 bg-amber-500/5 px-3 py-1.5 text-chrome-sm text-amber-200">
              Results are incomplete for {workspaceProblemsUnavailableLanguages.join(", ")}.
            </p>
          {/if}
          {#if workspaceProblemsLoading && !workspaceProblemsLoaded}
            <p class="flex items-center px-3 py-3 text-chrome-sm text-content-quiet"><LoaderCircle size={11} class="mr-1.5 animate-spin" />Loading project problems…</p>
          {:else if workspaceProblemCounts.total === 0}
            <p class="px-3 py-3 text-chrome-sm text-content-quiet">No problems found in this project.</p>
          {:else if workspaceProblemGroups.length === 0}
            <p class="px-3 py-3 text-chrome-sm text-content-quiet">No problems match the current filters.</p>
          {:else}
            {#each workspaceProblemGroups as group (group.path)}
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
                  onclick={() => void openWorkspaceProblem(problem)}
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
        {:else if contextPanel === "language"}
          <div class="flex flex-wrap items-center gap-2 border-b border-surface-500/20 bg-surface-900/55 px-3 py-2 text-chrome-sm text-content-secondary">
            <span class="font-medium">{activeTabLanguage}</span>
            {#if activeLanguageMatrix}
              <span class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs {activeLanguageMatrix.usable ? 'text-emerald-200' : 'text-rose-200'}">{activeLanguageMatrix.usable ? "usable" : "missing"}</span>
              {#if activeLanguageMatrix.command}
                <span class="font-mono text-chrome-xs text-content-quiet">{activeLanguageMatrix.command}</span>
              {/if}
              {#if activeLanguageMatrix.packageId}
                <span class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs text-content-quiet">pkg:{activeLanguageMatrix.packageId}</span>
              {/if}
            {/if}
            {#if latestLanguageSession}
              <span class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs {latestLanguageSession.phase === 'failed' ? 'text-rose-200' : latestLanguageSession.phase === 'ready' ? 'text-emerald-200' : 'text-amber-200'}">{latestLanguageSession.phase}</span>
              <span class="min-w-0 flex-1 truncate font-mono text-chrome-xs text-content-quiet" title={latestLanguageSession.language_root}>{latestLanguageSession.relative_root || "."}</span>
            {:else}
              <span class="min-w-0 flex-1 text-content-quiet">No workshop session snapshot yet</span>
            {/if}
            <button type="button" class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs hover:bg-surface-700" onclick={restartLanguageServer}>Restart</button>
          </div>
          {#if latestLanguageSession?.progress.some((progress) => !progress.done)}
            {#each latestLanguageSession.progress.filter((progress) => !progress.done) as progress (progress.token)}
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
          {:else if languageSessionLogs.length === 0}
            <p class="px-3 py-3 text-chrome-sm text-content-quiet">No language server output has been recorded.</p>
          {:else}
            <div class="font-mono text-chrome-xs" aria-label="Language server output">
              {#each languageSessionLogs as entry (`${entry.sessionId}:${entry.sequence}`)}
                <div class="grid grid-cols-[4.8rem_3.5rem_minmax(0,1fr)] gap-2 border-b border-surface-500/10 px-3 py-1 {entry.level === 'error' ? 'text-rose-200' : entry.level === 'warning' ? 'text-amber-200' : 'text-content-tertiary'}">
                  <span class="text-content-faint">{formatLanguageLogTime(entry.timestamp_ms)}</span>
                  <span class="truncate text-content-quiet">{entry.source}</span>
                  <span class="whitespace-pre-wrap break-words">{entry.message}</span>
                </div>
              {/each}
            </div>
          {/if}
        {:else if contextPanel === "references"}
          {#if references.length === 0}
            <p class="px-3 py-3 text-chrome-sm text-content-quiet">No other uses found.</p>
          {:else}
            {#each references as reference, index (`${reference.uri}:${reference.range?.start?.line}:${index}`)}
              {@const referencePath = pathFromUri(reference.uri)}
              {@const referenceLine = (reference.range?.start?.line ?? 0) + 1}
              <button
                type="button"
                class="flex w-full items-center gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60"
                onclick={async () => {
                  if (!referencePath) return;
                  setContextPanel(null);
                  await lmeWorkspace.openCodeFile(workId, referencePath, {
                    line: referenceLine,
                    groupId: shellTabs.activeGroupId,
                  });
                  undertakings.setSelection({ path: referencePath, line: referenceLine, entityId: null });
                  await tick();
                  editor?.revealLine(referenceLine);
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
              onclick={() => editor?.revealLine(symbolLine(symbol))}
            >
              <ListTree size={11} class="shrink-0 text-content-link/70" />
              <span class="min-w-0 flex-1 truncate text-chrome-sm text-content-secondary">{symbol.name}</span>
              <span class="font-mono text-chrome-xs text-content-quiet">{symbolLine(symbol)}</span>
            </button>
          {/each}
        {/if}
      </div>
    {/if}

    {#if outputOpen}
      <div class="flex max-h-52 shrink-0 flex-col border-t border-surface-500/30 bg-surface-950/80">
        <div class="flex items-center justify-between gap-2 border-b border-surface-500/20 px-2.5 py-1">
          <span class="text-chrome-xs font-medium uppercase tracking-[0.06em] text-content-quiet">
            {#if taskRun}Task: {taskRun.task.label}{:else}Output{/if}
            {#if taskRun?.state === "ready"}<span class="normal-case tracking-normal text-emerald-300/90"> · ready</span>
            {:else if runningTask}<span class="normal-case tracking-normal text-content-link"> · running</span>{/if}
            {#if taskOutputTruncated}<span class="normal-case tracking-normal text-amber-200/80"> · truncated</span>{/if}
          </span>
          <div class="flex items-center gap-1">
            {#if (taskReadyUrl || taskRun?.ready_url) && (taskRun?.state === "ready" || runningTask)}
              <button
                type="button"
                class="rounded px-1.5 py-0.5 text-chrome-xs text-emerald-200/90 hover:bg-emerald-500/10 disabled:opacity-40"
                disabled={previewOpening}
                onclick={() => void openTaskPreview()}
              >{previewOpening ? "Opening…" : "Open in Browser"}</button>
            {/if}
            {#if runningTask && (taskRun?.state === "running" || taskRun?.state === "ready")}
              <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-rose-200/90 hover:bg-rose-500/10" onclick={() => void stopDetectedTask()}>Stop</button>
            {/if}
            <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary" onclick={() => toggleOutput(false)}>Hide</button>
          </div>
        </div>
        {#if !taskLiveStdout && !taskLiveStderr && !runningTask && !taskRun}
          <p class="px-2.5 py-2 text-chrome-sm text-content-quiet">Run a project check to stream output here.</p>
        {:else}
          <pre class="min-h-0 flex-1 overflow-auto px-2.5 py-1.5 font-mono text-chrome-xs leading-relaxed text-content-tertiary whitespace-pre-wrap break-words" aria-label="Task output">{taskLiveStdout}{#if taskLiveStdout && taskLiveStderr}{"\n\n"}{/if}{#if taskLiveStderr}<span class="text-rose-200/90">{taskLiveStderr}</span>{/if}{#if !taskLiveStdout && !taskLiveStderr && runningTask}<span class="text-content-quiet">Waiting for output…</span>{/if}</pre>
          {#if taskLiveLocations.length}
            <div class="max-h-20 shrink-0 overflow-y-auto border-t border-surface-500/20">
              {#each taskLiveLocations.slice(0, 8) as location (`${location.path}:${location.line}:${location.column}:${location.message}`)}
                <button type="button" class="flex w-full items-center gap-2 border-b border-surface-500/10 px-2.5 py-1 text-left text-chrome-xs text-content-secondary hover:bg-surface-800/60" onclick={() => void openTaskLocation(location.path, location.line)}>
                  <span class="min-w-0 flex-1 truncate">{location.message || location.path}</span>
                  <span class="shrink-0 font-mono text-content-quiet">{location.path}:{location.line}</span>
                </button>
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    {/if}
    {#if taskResult}
      <div class="shrink-0 border-t {taskResult.success ? 'border-emerald-500/25 bg-emerald-950/20 text-emerald-200' : 'border-rose-500/30 bg-rose-950/25 text-rose-200'}">
      <button type="button" class="flex w-full items-center justify-between gap-2 px-2.5 py-1 text-left text-chrome-xs" title="Run this check again" onclick={() => void runDetectedTask()}>
        <span>{taskResult.success ? "Passed" : "Needs attention"} · {taskResult.task.label}</span>
        <span class="text-current">Rerun · {(taskResult.duration_ms / 1000).toFixed(1)}s{taskResult.exit_code != null ? ` · exit ${taskResult.exit_code}` : ""}</span>
      </button>
      {#each taskResult.locations.slice(0, 5) as location (`${location.path}:${location.line}:${location.column}`)}
        <button type="button" class="flex w-full items-center gap-2 border-t border-current/10 px-2.5 py-1 text-left text-chrome-xs hover:bg-white/5" onclick={() => void openTaskLocation(location.path, location.line)}>
          <span class="min-w-0 flex-1 truncate">{location.message || location.path}</span>
          <span class="shrink-0 font-mono">{location.path}:{location.line}</span>
        </button>
      {/each}
      </div>
    {/if}
    {#if searchOpen}
      <CodeWorkspaceSearch
        {workId}
        onOpenHit={openSearchHit}
        onClose={() => toggleSearch(false)}
        onApplied={async () => {
          await undertakings.refreshDetail();
          reconcileOpenFiles();
          try {
            quickFiles = (await getUndertakingSourceTree(workId)).files;
          } catch {
            /* tree refresh can retry later */
          }
        }}
      />
    {/if}
    {#if changesOpen}
      <CodeChangesPanel
        changes={forgeChanges}
        loading={changesLoading}
        error={changesError}
        selectedPath={selectedChangePath}
        fileDiff={changeFileDiff}
        fileLoading={changeFileLoading}
        fileError={changeFileError}
        restoreBusy={changeRestoreBusy}
        syncBusy={changeSyncBusy}
        syncMessage={changeSyncMessage}
        history={changesHistory}
        historyOpen={changesHistoryOpen}
        blameHunks={changesBlameHunks}
        blameOpen={changesBlameOpen}
        onSelectPath={(path) => void selectChangePath(path)}
        onOpenPath={(path, line) => void openTaskLocation(path, line ?? 1)}
        onRestorePath={(diff) => void restoreChangeFile(diff)}
        onRevertHunk={(diff, hunkIndex) => void revertChangeHunk(diff, hunkIndex)}
        onResolveConflict={(diff, resolution) => void resolveChangeConflict(diff, resolution)}
        onFetch={() => void runChangesSync("fetch")}
        onPull={() => void runChangesSync("pull")}
        onPush={() => void runChangesSync("push")}
        onSync={() => void runChangesSync("sync")}
        onCheckpoint={() => void sealChangesForReview()}
        onToggleHistory={() => void toggleChangesHistory()}
        onToggleBlame={() => void toggleChangesBlame()}
        onClose={() => void toggleChanges(false)}
        onRefresh={() => void refreshForgeChanges()}
      />
    {/if}
    {#if testsOpen}
      <div class="max-h-44 shrink-0 overflow-y-auto border-t border-surface-500/25 bg-surface-950/90">
        <div class="sticky top-0 flex items-center justify-between bg-surface-950 px-2.5 py-1 text-chrome-xs uppercase tracking-wider text-content-quiet"><span>Project tests</span><span>{projectTests.length}</span></div>
        {#if projectTests.length === 0}
          <p class="px-3 py-3 text-chrome-sm text-content-quiet">No individual tests were discovered. The project test command still works.</p>
        {:else}
          {#each projectTests as test (test.id)}
            <div class="flex items-center border-t border-surface-500/15">
              <button type="button" class="min-w-0 flex-1 truncate px-3 py-1.5 text-left text-chrome-sm text-content-secondary hover:bg-surface-800/60" onclick={() => void openTaskLocation(test.path, test.line)}>{test.label}<span class="ml-2 font-mono text-chrome-xs text-content-faint">{test.path}:{test.line}</span></button>
              <button type="button" class="mr-2 rounded px-1.5 py-0.5 text-chrome-xs text-content-link hover:bg-surface-800 disabled:opacity-40" disabled={runningTask} onclick={() => void runDetectedTask(test)}>Run</button>
            </div>
          {/each}
        {/if}
      </div>
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
                onclick={() => void toggleTerminalDock(true)}
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
                onclick={() => void showQuickOpen()}
              >Open file</button>
              {#if terminalAvailable}
                <button
                  type="button"
                  class="rounded border border-surface-500/40 px-3 py-1.5 text-chrome-md text-surface-200 hover:bg-surface-800"
                  disabled={dockBusy}
                  onclick={() => void toggleTerminalDock(true)}
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
    worktreeRoot={workspaceRoot ?? context?.worktree ?? null}
    title="Terminal"
    onClose={() => {
      terminalDockOpen = false;
      if (workId) {
        codeWorkbenchState.setTerminalOpen(workId, false);
        codeWorkspace.scheduleLayoutPersist(workId);
      }
    }}
    onPopOut={() => void popOutTerminal()}
  />
</section>

<style>
  .code-source-editor--fill {
    border: 0;
    border-radius: 0;
    background: transparent;
  }

  .code-editor-chrome {
    min-height: 30px;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
    background: transparent;
  }

  .code-editor-chrome-title {
    max-width: 8.5rem;
    flex-shrink: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 92%,
      rgb(var(--theme-text-secondary))
    );
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0;
    line-height: 1.2;
  }

  .code-editor-chrome-sep {
    flex-shrink: 0;
    color: color-mix(in srgb, rgb(var(--theme-text)) 40%, transparent);
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 500;
    line-height: 1;
  }

  .code-editor-chrome-phase {
    flex-shrink: 0;
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 55%,
      rgb(var(--theme-text-secondary))
    );
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 12px;
    font-weight: 400;
  }

  .code-editor-chrome-divider {
    margin: 0 0.15rem;
    height: 1rem;
    width: 1px;
    flex-shrink: 0;
    background: rgb(var(--color-surface-500) / 0.35);
  }

  :global(.code-chrome-menu-item) {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    padding: 0.4rem 0.5rem;
    color: rgb(var(--theme-text));
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 400;
    text-align: left;
    cursor: pointer;
  }

  :global(.code-chrome-menu-item:hover:not(:disabled)) {
    background: rgb(var(--color-surface-800) / 0.7);
  }

  :global(.code-chrome-menu-item:disabled) {
    opacity: 0.4;
    cursor: default;
  }

  :global(.code-chrome-menu-item--warn) {
    color: rgb(var(--theme-warning));
  }

  :global(.code-chrome-menu-meta) {
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 50%,
      rgb(var(--theme-text-secondary))
    );
    font-size: 12px;
  }

  :global(.code-chrome-menu-sep) {
    margin: 0.25rem 0.35rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.25);
  }

  :global(.code-chrome-menu-field) {
    display: block;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.2);
    padding: 0.4rem 0.5rem 0.5rem;
  }

  :global(.code-chrome-menu-field-label) {
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 45%,
      transparent
    );
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  :global(.code-chrome-menu-select) {
    margin-top: 0.3rem;
    width: 100%;
    border: 0;
    border-radius: 0.3rem;
    background: rgb(var(--color-surface-800));
    padding: 0.3rem 0.4rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 12px;
    outline: none;
  }

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
        <input bind:this={quickInput} class="min-w-0 flex-1 bg-transparent py-2.5 text-sm text-surface-100 outline-none" placeholder="Fuzzy file, @symbol, or :line" bind:value={quickQuery} oninput={() => { quickIndex = 0; void refreshQuickSymbols(); }} onkeydown={(event) => {
          if (event.key === "ArrowDown") { event.preventDefault(); quickIndex = Math.min(quickIndex + 1, quickResultCount - 1); }
          if (event.key === "ArrowUp") { event.preventDefault(); quickIndex = Math.max(quickIndex - 1, 0); }
          if (event.key === "Enter") { event.preventDefault(); chooseQuickResult(); }
        }} />
        <span class="text-chrome-xs text-content-faint">⌘P</span>
      </div>
      <div class="max-h-[50vh] overflow-y-auto py-1">
        {#if quickMode === "line"}
          <button type="button" class="flex w-full items-center gap-2 px-3 py-2 text-left text-content-secondary hover:bg-surface-800" onclick={chooseQuickLine}>
            <span class="font-mono text-xs text-content-link">:{quickQuery.slice(1).trim() || "line"}</span>
            <span class="text-chrome-sm text-content-quiet">Go to a line in {activeTab?.title}</span>
          </button>
        {:else if quickMode === "symbol" && quickSymbolResults.length === 0}
          <p class="px-3 py-3 text-xs text-content-quiet">No matching project symbols.</p>
        {:else if quickMode === "symbol"}
          {#each quickSymbolResults as symbol, index (`${symbol.name}:${symbol.location?.uri}:${symbol.location?.range?.start?.line}`)}
            <button type="button" class="flex w-full items-center gap-2 px-3 py-1.5 text-left {index === quickIndex ? 'bg-surface-800 text-surface-100' : 'text-content-tertiary hover:bg-surface-900'}" onmouseenter={() => (quickIndex = index)} onclick={() => void chooseQuickSymbol(symbol)}>
              <ListTree size={12} class="shrink-0 opacity-65" />
              <span class="min-w-0 flex-1 truncate text-xs">{symbol.name}</span>
              <span class="min-w-0 max-w-[60%] truncate font-mono text-chrome-xs text-content-faint">{symbol.containerName ?? pathFromUri(symbol.location?.uri) ?? ""}</span>
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
              <span class="min-w-0 max-w-[60%] truncate font-mono text-chrome-xs text-content-faint">{file.path}</span>
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
            <p class="font-mono text-chrome-xs text-content-quiet">{conflictTab.path}</p>
            <p class="mt-0.5 text-chrome-sm text-content-quiet">Draft vs project — same comparison chrome as Review.</p>
          </div>
          <button type="button" class="rounded p-1 text-content-quiet hover:text-surface-100" aria-label="Close comparison" onclick={() => (comparingTabId = null)}><X size={13} /></button>
        </header>
        <div class="min-h-0 flex-1 overflow-auto px-3 py-2">
          <DiffStack files={conflictFiles} mode="side" showJumpList={false} />
        </div>
        <footer class="flex justify-end gap-2 border-t border-surface-500/30 px-3 py-2">
          <button type="button" class="rounded px-2 py-1 text-chrome-sm text-content-secondary hover:bg-surface-800" onclick={() => useProjectVersion(conflictTab)}>Use project version</button>
          <button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-chrome-sm font-medium text-white" onclick={() => keepDraft(conflictTab)}>Keep my draft</button>
        </footer>
      </div>
    </div>
  {/if}
{/if}

{#if refactorPreview?.workId === workId}
  {@const refactorPlan = refactorPreview.plan}
  {@const resourceOperationCount = refactorPlan.operations.filter((operation) => operation.kind !== "write").length}
  <div class="fixed inset-0 z-[128] flex items-center justify-center p-4">
    <button
      type="button"
      class="absolute inset-0 bg-black/60"
      aria-label="Cancel refactor"
      disabled={refactorApplying}
      onclick={() => {
        if (!refactorApplying) refactorPreview = null;
      }}
    ></button>
    <div
      class="relative flex max-h-[90vh] w-full max-w-6xl flex-col overflow-hidden rounded-lg border border-surface-500/50 bg-surface-950 shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-label="Review refactor"
      aria-busy={refactorApplying}
      tabindex="-1"
    >
      <header class="flex items-start justify-between gap-3 border-b border-surface-500/30 px-4 py-3">
        <div class="min-w-0">
          <p class="text-sm font-medium text-surface-100">Review refactor</p>
          <p class="mt-0.5 text-chrome-sm leading-relaxed text-content-quiet">
            Nothing changes until you apply this preview. The workshop verifies every file snapshot and commits the complete edit atomically.
          </p>
          <div class="mt-2 flex flex-wrap items-center gap-1.5 text-chrome-xs text-content-tertiary">
            <span class="rounded bg-surface-800 px-1.5 py-0.5">{refactorPlan.operations.length} {refactorPlan.operations.length === 1 ? "operation" : "operations"}</span>
            {#if resourceOperationCount > 0}
              <span class="rounded bg-primary-950/60 px-1.5 py-0.5 text-primary-200">{resourceOperationCount} file {resourceOperationCount === 1 ? "operation" : "operations"}</span>
            {/if}
            {#each refactorPlan.annotationLabels as label (label)}
              <span class="rounded bg-amber-950/55 px-1.5 py-0.5 text-amber-200">{label}</span>
            {/each}
          </div>
        </div>
        <button
          type="button"
          class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-100 disabled:opacity-40"
          aria-label="Cancel refactor"
          disabled={refactorApplying}
          onclick={() => (refactorPreview = null)}
        ><X size={14} /></button>
      </header>
      {#if surfaceError}
        <p class="shrink-0 border-b border-amber-500/30 bg-amber-950/25 px-4 py-2 text-chrome-sm text-amber-100">
          {humanizeForgeMessage(surfaceError)}
        </p>
      {/if}
      <div class="min-h-0 flex-1 overflow-auto px-4 py-3">
        <DiffStack
          files={refactorDiffFiles}
          bind:mode={refactorDiffMode}
          showJumpList={true}
          busy={refactorApplying}
          title="Proposed project changes"
          subtitle="Create, rename, and delete operations are included in the same guarded transaction as text edits."
        />
      </div>
      <footer class="flex items-center justify-between gap-3 border-t border-surface-500/30 px-4 py-3">
        <p class="text-chrome-xs text-content-quiet">If any file changed since this preview was built, Apply stops without changing the project.</p>
        <div class="flex shrink-0 items-center gap-2">
          <button
            type="button"
            class="rounded px-2.5 py-1.5 text-chrome-sm text-content-tertiary hover:bg-surface-800 disabled:opacity-40"
            disabled={refactorApplying}
            onclick={() => (refactorPreview = null)}
          >Cancel</button>
          <button
            type="button"
            class="inline-flex items-center gap-1.5 rounded bg-primary-500/80 px-2.5 py-1.5 text-chrome-sm font-medium text-white hover:bg-primary-500 disabled:opacity-40"
            disabled={refactorApplying}
            onclick={() => void applyRefactorPreview()}
          >{#if refactorApplying}<LoaderCircle size={11} class="animate-spin" />Applying…{:else}Apply refactor{/if}</button>
        </div>
      </footer>
    </div>
  </div>
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
        <p class="text-chrome-sm text-content-quiet">Applies across the project when the language server supports it.</p>
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
        <button type="button" class="rounded px-2 py-1 text-chrome-sm text-content-tertiary hover:bg-surface-800" onclick={cancelInlineRename}>Cancel</button>
        <button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-chrome-sm font-medium text-white disabled:opacity-40" disabled={!renameDraft.trim() || languageActionRunning} onclick={() => void commitInlineRename()}>Rename</button>
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
    if (refactorPreview) {
      if (event.key === "Escape" && !refactorApplying) {
        event.preventDefault();
        refactorPreview = null;
      }
      return;
    }
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
      if (canInvokeCodeSaveShortcut({ editable, canBeginEdit })) void saveAll();
      return;
    }
    if (command && event.key.toLowerCase() === "s") {
      event.preventDefault();
      if (
        canInvokeCodeSaveShortcut({ editable, canBeginEdit }) &&
        activeTab &&
        codeWorkspace.isDirty(activeTab)
      ) {
        void saveTab(activeTab);
      }
      return;
    }
    if (command && event.key === "`") {
      event.preventDefault();
      void toggleTerminalDock();
      return;
    }
    if (command && event.shiftKey && event.key.toLowerCase() === "f") {
      event.preventDefault();
      toggleSearch(true);
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
    if (event.key === "Escape" && contextPanel) setContextPanel(null);
  }}
/>
