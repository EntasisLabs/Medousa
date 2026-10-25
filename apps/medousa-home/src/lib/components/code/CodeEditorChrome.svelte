<script lang="ts">
  import type { Snippet } from "svelte";
  import {
    CircleAlert,
    ArrowLeft,
    ArrowRight,
    Pencil,
    Save,
    Ellipsis,
    Square,
    SquareTerminal,
    GitPullRequestArrow,
    Play,
    UserRound,
    Search,
    GitBranch,
  } from "@lucide/svelte";
  import CodeCommandPicker from "./CodeCommandPicker.svelte";
  import CodeMarkdownViewControls from "./CodeMarkdownViewControls.svelte";
  import type { CodeMarkdownMode } from "$lib/code/codeMarkdownDocument";
  import CodeBreadcrumbs from "$lib/components/code/CodeBreadcrumbs.svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import type { CodeChangesController } from "$lib/code/codeChangesController.svelte";
  import type { CodeProblemsController } from "$lib/code/codeProblemsController.svelte";
  import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
  import { languageSupportsLsp } from "$lib/code/codeEditorLanguageRegistry";
  import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
  import { codeSyntaxThemePreference } from "$lib/stores/codeSyntaxThemePreference.svelte";
  import { titleWithShortcut } from "$lib/utils/keyboardShortcutsCatalog";

  type ActiveTab = { path: string; loading: boolean; language: string; preview?: boolean; markdownMode?: CodeMarkdownMode };

  interface Props {
    workId: string;
    activeTab: ActiveTab | null;
    onMarkdownModeChange: (mode: CodeMarkdownMode) => void | Promise<void>;
    projectTitle?: string | null;
    phaseLabel?: string | null;
    reviewAvailable?: boolean;
    onOpenReview?: () => void;
    agentRunning?: boolean;
    agentLabel?: string;
    onStopAgent?: () => void;
    onResumeEditing?: () => void;
    problems: CodeProblemsController;
    searchOpen: boolean;
    onToggleSearch: () => void;
    changes: CodeChangesController;
    terminalAvailable?: boolean;
    terminalDockOpen?: boolean;
    onToggleTerminal: () => void;
    tasks: CodeTasksController;
    agentHasControl: boolean;
    busy: boolean;
    editable: boolean;
    canBeginEdit: boolean;
    dirty: boolean;
    onReclaimHuman: () => void;
    onStartEditing: () => void;
    onSave: () => void;
    savingFile: boolean;
    onOpenFind: () => void;
    hasLspClient: boolean;
    onShowOutline: () => void;
    onToggleWorld?: () => void;
    worldOpen?: boolean;
    onReload: () => void;
    wordWrap: boolean;
    onToggleWordWrap: () => void;
    showLineNumbers: boolean;
    onToggleLineNumbers: () => void;
    fontSize: number;
    onCycleFontSize: () => void;
    onOpenSyntaxTheme: () => void;
    tabSizePref: number;
    onCycleTabSize: () => void;
    canFormat: boolean;
    canCodeAction: boolean;
    languageActionRunning: boolean;
    onLanguageAction: (action: "format" | "organize_imports") => void;
    onRestartLanguage: () => void;
    onShowLanguageLogs: () => void;
    lspError: string | null;
    repairingLanguage: boolean;
    onRepairLanguage: () => void;
    projectMenu?: Snippet;
    onNavigate: (direction: -1 | 1) => void;
    onPathSegment: (path: string, isFile: boolean) => void;
  }

  let {
    workId,
    activeTab,
    onMarkdownModeChange,
    projectTitle = null,
    phaseLabel = null,
    reviewAvailable = false,
    onOpenReview,
    agentRunning = false,
    agentLabel = "agent",
    onStopAgent,
    onResumeEditing,
    problems,
    searchOpen,
    onToggleSearch,
    changes,
    terminalAvailable = false,
    terminalDockOpen = false,
    onToggleTerminal,
    tasks,
    agentHasControl,
    busy,
    editable,
    canBeginEdit,
    dirty,
    onReclaimHuman,
    onStartEditing,
    onSave,
    savingFile,
    onOpenFind,
    hasLspClient,
    onShowOutline,
    onToggleWorld,
    worldOpen = false,
    onReload,
    wordWrap,
    onToggleWordWrap,
    showLineNumbers,
    onToggleLineNumbers,
    fontSize,
    onCycleFontSize,
    onOpenSyntaxTheme,
    tabSizePref,
    onCycleTabSize,
    canFormat,
    canCodeAction,
    languageActionRunning,
    onLanguageAction,
    onRestartLanguage,
    onShowLanguageLogs,
    lspError,
    repairingLanguage,
    onRepairLanguage,
    projectMenu,
    onNavigate,
    onPathSegment,
  }: Props = $props();

  function navigate(direction: -1 | 1) {
    onNavigate(direction);
  }
  function toggleSearch() {
    onToggleSearch();
  }
  function toggleTerminalDock() {
    onToggleTerminal();
  }
  function showOutline() {
    onShowOutline();
  }
  function reload() {
    onReload();
  }
  function toggleWordWrap() {
    onToggleWordWrap();
  }
  function toggleLineNumbers() {
    onToggleLineNumbers();
  }
  function cycleFontSize() {
    onCycleFontSize();
  }
  function openSyntaxThemeSettings() {
    onOpenSyntaxTheme();
  }
  function cycleTabSize() {
    onCycleTabSize();
  }
  function runLanguageAction(action: "format" | "organize_imports") {
    onLanguageAction(action);
  }
  function restartLanguageServer() {
    onRestartLanguage();
  }
  function showLanguageLogs() {
    onShowLanguageLogs();
  }
  function repairLanguageSupport() {
    onRepairLanguage();
  }
  // Give long filenames/deep paths the same quieter actions as a narrow pane.
  const compactActions = $derived(!!activeTab && (
    (activeTab.path.split("/").at(-1)?.length ?? 0) > 28 || activeTab.path.length > 64
  ));
  const activeTabLanguage = $derived(activeTab?.language ?? "");
</script>
  <header class="code-editor-chrome relative z-20 flex shrink-0 items-center gap-1.5 border-b border-surface-500/30 px-3" class:code-editor-chrome--document={!!activeTab} class:code-editor-chrome--compact={compactActions}>
    <div class="code-editor-location flex min-w-0 flex-1 items-center gap-1">
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
          onPathSegment={onPathSegment}
        />
        {#if dirty || savingFile}<span class="code-file-state" title={savingFile ? "Saving…" : "Unsaved changes"} role="status" aria-label={savingFile ? "Saving file" : "Unsaved changes"}>●</span>{/if}
      {/if}
    </div>
    <div class="code-editor-chrome-actions flex shrink-0 items-center gap-0.5">
      {#if activeTab?.language === "markdown" && !activeTab.preview}
        <CodeMarkdownViewControls compact={compactActions} mode={activeTab.markdownMode ?? "source"} disabled={activeTab.loading} onchange={onMarkdownModeChange}/>
      {/if}
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
            else onReclaimHuman();
          }}
        ><Play size={14} strokeWidth={1.75} /></button>
      {/if}
      {#if tasks.projectTasks.length > 0 || activeTab}
        <CodeCommandPicker {tasks} path={activeTab?.path ?? ""} variant="run" runDisabled={agentHasControl || busy}
              runTitle={agentHasControl
                ? `Resume editing before running ${tasks.selectedTask?.label ?? "this command"}`
                : !tasks.taskAvailable(tasks.selectedTask)
                  ? tasks.taskRepair(tasks.selectedTask) ?? `${tasks.selectedTask?.label ?? "Command"} is unavailable`
                : tasks.selectedTask
                  ? `Run ${tasks.selectedTask.label}: ${tasks.selectedTask.argv.join(" ")}`
                  : "Choose a project command"}/>
      {/if}
      {#if activeTab}
        {#if terminalAvailable}
          <button type="button" class="scripts-workbench-toolbar-btn code-terminal-button {terminalDockOpen ? 'scripts-workbench-toolbar-btn-active' : ''}" title={titleWithShortcut("Toggle terminal", "code-terminal")} aria-label="Toggle terminal" aria-pressed={terminalDockOpen} onclick={() => void toggleTerminalDock()}><SquareTerminal size={14} strokeWidth={1.75}/><span class="code-terminal-label">Terminal</span></button>
        {/if}
        <button
          type="button"
          class="scripts-workbench-toolbar-btn code-secondary-action {problems.panel === 'problems' ? 'scripts-workbench-toolbar-btn-active' : ''}"
          title={problems.counts.total ? `Problems · ${problems.counts.errors} errors, ${problems.counts.warnings} warnings` : "Problems"}
          aria-label={problems.counts.total ? `Show issues (${problems.counts.total})` : "Show issues"}
          aria-pressed={problems.panel === "problems"}
          onclick={() => void problems.showProblems()}
        ><CircleAlert size={14} strokeWidth={1.75} />{#if problems.counts.total}<span class="code-problem-count" class:code-problem-count--error={problems.counts.errors > 0}>{problems.counts.total}</span>{/if}</button>
        <button
          type="button"
          class="scripts-workbench-toolbar-btn code-secondary-action {searchOpen ? 'scripts-workbench-toolbar-btn-active' : ''}"
          title={titleWithShortcut("Search in files", "code-search")}
          aria-label="Search in files"
          aria-pressed={searchOpen}
          onclick={() => toggleSearch()}
        ><Search size={14} strokeWidth={1.75} /></button>
        <button
          type="button"
          class="scripts-workbench-toolbar-btn code-secondary-action {changes.open ? 'scripts-workbench-toolbar-btn-active' : ''}"
          title="Changes"
          aria-label="Show changes"
          aria-pressed={changes.open}
          onclick={() => void changes.toggle()}
        ><GitBranch size={14} strokeWidth={1.75} /></button>

        {#if agentHasControl && !agentRunning}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={busy}
            onclick={onReclaimHuman}
            aria-label="Resume editing"
            title="Resume editing — take the file back from the agent"
          ><UserRound size={14} strokeWidth={1.75} /></button>
        {:else if !editable && canBeginEdit}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={busy}
            onclick={onStartEditing}
            aria-label="Edit file"
            title="Start editing (or just type)"
          ><Pencil size={14} strokeWidth={1.75} /></button>
        {:else if dirty || savingFile}
          <button
            type="button"
            class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
            disabled={!editable || !dirty || savingFile}
            onclick={onSave}
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
              <Ellipsis size={15} strokeWidth={1.75} />
            </button>
          {/snippet}
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={() => void problems.showProblems()}><span>Problems</span><span class="code-chrome-menu-meta">{problems.counts.total}</span></button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={toggleSearch}>Search in files</button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={() => void changes.toggle()}>Changes</button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" disabled={!editable || !dirty || savingFile} onclick={onSave}>Save file</button>
          <div class="code-chrome-menu-sep" role="separator"></div>
          <button type="button" role="menuitem" class="code-chrome-menu-item" title={titleWithShortcut("Find in file", "code-find")} onclick={onOpenFind}>Find in file</button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={() => tasks.toggleOutput()}>
            <span>Output</span>
            <span class="code-chrome-menu-meta">{tasks.outputOpen ? "On" : "Off"}</span>
          </button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" disabled={!hasLspClient} onclick={() => void showOutline()}>
            <span>{titleWithShortcut("Structure", "code-structure")}</span>
          </button>
          <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={onToggleWorld}>
            <span>Understand this code</span>
            <span class="code-chrome-menu-meta">{worldOpen ? "On" : "Off"}</span>
          </button>
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
          {#if tasks.projectTasks.some((task) => task.kind === "test")}
            <button type="button" role="menuitem" class="code-chrome-menu-item" onclick={() => void tasks.toggleTests()}>
              <span>Discovered tests</span>
              <span class="code-chrome-menu-meta">{tasks.testsOpen ? "Hide" : "Show"}</span>
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
          {#if projectMenu}
            <div class="code-chrome-menu-sep" role="separator"></div>
            {@render projectMenu()}
          {/if}
        </OverflowMenu>
      {:else if projectMenu}
        <OverflowMenu
          label="Project options"
          title="Project options"
          panelClass="w-[min(16rem,calc(100vw-2rem))] rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
        >
          {@render projectMenu()}
        </OverflowMenu>
      {/if}
    </div>
  </header>

<style>
  .code-editor-chrome {
    min-height: 44px;
    padding-block:6px;
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
    color: rgb(var(--theme-text-tertiary));
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 12px;
    font-weight: 400;
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

  .code-editor-chrome-actions > button { display: inline-flex; align-items: center; }
  .code-editor-chrome :global(.scripts-workbench-toolbar-btn) { min-width:28px; height:28px; padding-inline:6px; gap:.35rem; }
  .code-terminal-label { font-size:12px; }
  .code-file-state { flex-shrink:0; color:rgb(var(--theme-text-tertiary)); font-size:8px; }
  .code-problem-count { color:rgb(var(--theme-warning)); font-size:11px; font-variant-numeric:tabular-nums; }
  .code-problem-count--error { color:rgb(var(--theme-error)); }
  .code-editor-chrome--compact .code-editor-chrome-title,
  .code-editor-chrome--compact .code-editor-chrome-sep,
  .code-editor-chrome--compact .code-terminal-label,
  .code-editor-chrome--compact .code-secondary-action { display:none !important; }
  .code-editor-chrome--compact :global(.command-run-button span) { max-width:5rem; }
  @container code-editor (max-width:1050px) {
    .code-editor-chrome--document .code-editor-chrome-title, .code-editor-chrome--document .code-editor-chrome-sep { display:none; }
  }
  @container code-editor (max-width:700px) {
    .code-terminal-label { display:none; }
    .code-secondary-action { display:none !important; }
  }
  @container code-editor (max-width:500px) {
    .code-editor-chrome { flex-wrap:wrap; row-gap:4px; }
    .code-editor-location { flex-basis:100%; }
    .code-editor-chrome-actions { margin-left:auto; max-width:100%; flex-wrap:wrap; justify-content:flex-end; }
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

</style>
