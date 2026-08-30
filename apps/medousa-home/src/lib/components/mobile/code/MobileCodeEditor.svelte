<script lang="ts">
  import { ArrowLeft, ArrowRight, ChevronDown, ChevronUp, Files, X } from "@lucide/svelte";
  import {
    SearchQuery,
    closeSearchPanel,
    findNext,
    findPrevious,
    setSearchQuery,
  } from "@codemirror/search";
  import type { EditorView } from "@codemirror/view";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import CodeFileIcon from "$lib/components/lme/explorers/CodeFileIcon.svelte";
  import { pathToFileUri } from "$lib/code/codeDocumentUri";
  import { decideCodeSave, CODE_SAVE_NO_LEASE_ERROR, CODE_SAVE_PREVIEW_ERROR } from "$lib/code/codeSaveGate";
  import {
    canStartHumanEditing,
    humanizeForgeMessage,
    saveUndertakingSource,
    startHumanEditingSession,
  } from "$lib/forge";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
  import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { openMobileCodeFile } from "$lib/utils/mobileCodeOpen";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    workId: string;
  }

  let { workId }: Props = $props();

  let editor = $state<{
    getValue: () => string;
    flushChanges: () => void;
    getView: () => EditorView | undefined;
  } | null>(null);
  let saving = $state(false);
  let beginningEdit = $state(false);
  let surfaceError = $state<string | null>(null);
  let findOpen = $state(false);
  let findQuery = $state("");
  let findInput = $state<HTMLInputElement | null>(null);
  let switcherSheetEl = $state<HTMLDivElement | null>(null);
  let switcherHeaderEl = $state<HTMLElement | null>(null);
  let beginEditPromise: Promise<void> | null = null;

  const tab = $derived(codeWorkspace.activeFor(workId));
  const tabs = $derived(codeWorkspace.orderedTabsFor(workId));
  const dirty = $derived(tab ? codeWorkspace.isDirty(tab) : false);
  const workspaceRoot = $derived(undertakings.detail?.environment?.worktree ?? null);
  const documentUri = $derived(
    tab && workspaceRoot
      ? pathToFileUri(`${workspaceRoot.replace(/[\\/]$/, "")}/${tab.path}`)
      : null,
  );
  const canBeginEdit = $derived(
    canStartHumanEditing(undertakings.detail?.allowed_actions),
  );
  const hasLease = $derived(
    Boolean(
      undertakings.active?.workId === workId &&
        undertakings.active.leaseId &&
        undertakings.active.leaseGeneration != null,
    ),
  );
  const switcherOpen = $derived(mobileCodeWorkspaceState.fileSwitcherOpen);
  const fileLabel = $derived(tab?.title || tab?.path.split("/").at(-1) || "No file open");

  $effect(() => {
    const onSave = () => void save();
    const onFind = () => openFindBar();
    window.addEventListener("medousa-mobile-code-save", onSave);
    window.addEventListener("medousa-mobile-code-find", onFind);
    return () => {
      window.removeEventListener("medousa-mobile-code-save", onSave);
      window.removeEventListener("medousa-mobile-code-find", onFind);
    };
  });

  $effect(() => {
    if (!switcherOpen && !findOpen) return;
    return registerMobileBackHandler(() => {
      if (findOpen) {
        closeFindBar();
        return true;
      }
      mobileCodeWorkspaceState.fileSwitcherOpen = false;
      return true;
    });
  });

  $effect(() => {
    if (!switcherOpen || !switcherSheetEl) return;
    return attachMobileSheetGestures(switcherSheetEl, switcherHeaderEl, {
      onDismiss: dismissFileSwitcher,
      swipeBack: false,
    });
  });

  function closeFileSwitcher() {
    mobileCodeWorkspaceState.fileSwitcherOpen = false;
  }

  function dismissFileSwitcher() {
    haptic("light");
    closeFileSwitcher();
  }

  function view(): EditorView | undefined {
    return editor?.getView();
  }

  function applyFindQuery() {
    const current = view();
    if (!current) return;
    closeSearchPanel(current);
    current.dispatch({
      effects: setSearchQuery.of(
        new SearchQuery({ search: findQuery, caseSensitive: false }),
      ),
    });
  }

  function openFindBar() {
    findOpen = true;
    queueMicrotask(() => findInput?.focus());
    applyFindQuery();
  }

  function closeFindBar() {
    findOpen = false;
    const current = view();
    if (current) {
      closeSearchPanel(current);
      current.dispatch({
        effects: setSearchQuery.of(new SearchQuery({ search: "" })),
      });
    }
  }

  function stepFind(direction: "next" | "previous") {
    haptic("light");
    applyFindQuery();
    const current = view();
    if (!current || !findQuery.trim()) return;
    if (direction === "next") findNext(current);
    else findPrevious(current);
  }

  async function startEditing() {
    const detail = undertakings.detail;
    if (!detail || !canStartHumanEditing(detail.allowed_actions)) return;
    if (beginEditPromise) {
      await beginEditPromise;
      return;
    }
    beginningEdit = true;
    beginEditPromise = (async () => {
      const begun = await startHumanEditingSession(detail.id, detail.allowed_actions);
      undertakings.setActiveFromItem(begun.item, {
        leaseId: begun.lease.lease_id,
        leaseGeneration: begun.lease.generation,
        executorKind: "human",
      });
      codeWorkspace.setLease(workId, begun.lease);
      await undertakings.refreshDetail();
    })();
    try {
      await beginEditPromise;
    } finally {
      beginEditPromise = null;
      beginningEdit = false;
    }
  }

  async function save() {
    if (!tab) return;
    if (editor) {
      const live = editor.getValue();
      if (live !== tab.draft) codeWorkspace.updateDraft(tab.tabId, live);
      editor.flushChanges();
    }
    const decision = decideCodeSave({
      preview: Boolean(tab.preview),
      dirty: codeWorkspace.isDirty(tab),
      savingFile: saving,
      hasLease,
      canBeginEdit,
      beginningEdit: beginningEdit || Boolean(beginEditPromise),
    });
    if (decision.action === "noop") return;
    if (decision.action === "reject") {
      surfaceError = decision.reason === "preview" ? CODE_SAVE_PREVIEW_ERROR : CODE_SAVE_NO_LEASE_ERROR;
      return;
    }
    try {
      if (decision.action === "await-lease" && beginEditPromise) await beginEditPromise;
      if (decision.action === "begin-then-save" || !hasLease) await startEditing();
    } catch (err) {
      surfaceError = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
      return;
    }
    const leaseId = undertakings.active?.leaseId;
    const generation = undertakings.active?.leaseGeneration;
    const current = codeWorkspace.activeFor(workId);
    if (!leaseId || generation == null || !current || !codeWorkspace.isDirty(current)) return;
    saving = true;
    surfaceError = null;
    try {
      const next = await saveUndertakingSource(workId, {
        path: current.path,
        content: current.draft,
        lease_id: leaseId,
        generation,
        expected_digest: current.digest,
      });
      codeWorkspace.acceptSaved(current.tabId, next);
    } catch (err) {
      const message = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
      codeWorkspace.setError(current.tabId, message);
      surfaceError = message;
    } finally {
      saving = false;
    }
  }

  async function onDraftChanged(value: string) {
    if (!tab) return;
    codeWorkspace.updateDraft(tab.tabId, value);
    if (!hasLease && canBeginEdit) await startEditing();
  }

  async function navigate(direction: -1 | 1) {
    haptic("light");
    await codeWorkspace.navigate(workId, direction);
  }

  async function switchTo(path: string) {
    haptic("light");
    mobileCodeWorkspaceState.fileSwitcherOpen = false;
    await openMobileCodeFile(workId, path, { origin: "files" });
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="flex h-11 shrink-0 items-center gap-0.5 border-b border-surface-500/25 px-1">
    <button
      type="button"
      class="mobile-icon-btn"
      aria-label="Go back"
      disabled={!codeWorkspace.canNavigate(workId, -1)}
      onclick={() => void navigate(-1)}
    ><ArrowLeft size={18} /></button>
    <button
      type="button"
      class="mobile-icon-btn"
      aria-label="Go forward"
      disabled={!codeWorkspace.canNavigate(workId, 1)}
      onclick={() => void navigate(1)}
    ><ArrowRight size={18} /></button>
    <button
      type="button"
      class="flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-2 py-2 text-left active:bg-surface-800"
      onclick={() => {
        haptic("light");
        mobileCodeWorkspaceState.fileSwitcherOpen = true;
      }}
    >
      {#if tab}
        <CodeFileIcon path={tab.path} size={14} />
      {/if}
      <span class="truncate font-mono text-[13px] text-content-secondary">{fileLabel}</span>
      {#if dirty}
        <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-300" title="Unsaved"></span>
      {/if}
      <ChevronDown size={14} class="ml-auto shrink-0 text-content-quiet" />
    </button>
  </div>

  {#if findOpen}
    <div class="flex h-12 shrink-0 items-center gap-1 border-b border-surface-500/30 bg-surface-900 px-2">
      <input
        bind:this={findInput}
        class="min-h-11 min-w-0 flex-1 rounded-md border border-surface-500/40 bg-surface-950 px-3 text-sm text-content-secondary outline-none"
        placeholder="Find in file"
        bind:value={findQuery}
        oninput={() => applyFindQuery()}
        onkeydown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            stepFind(event.shiftKey ? "previous" : "next");
          }
          if (event.key === "Escape") {
            event.preventDefault();
            closeFindBar();
          }
        }}
      />
      <button type="button" class="mobile-icon-btn" aria-label="Previous match" onclick={() => stepFind("previous")}>
        <ChevronUp size={18} />
      </button>
      <button type="button" class="mobile-icon-btn" aria-label="Next match" onclick={() => stepFind("next")}>
        <ChevronDown size={18} />
      </button>
      <button type="button" class="mobile-icon-btn" aria-label="Close find" onclick={closeFindBar}>
        <X size={18} />
      </button>
    </div>
  {/if}

  {#if surfaceError || tab?.error}
    <p class="shrink-0 px-3 py-1.5 text-[12px] text-amber-200">
      {surfaceError || tab?.error}
    </p>
  {/if}

  <div class="mobile-code-editor-canvas relative min-h-0 flex-1 overflow-hidden">
    {#if tab}
      {#key `${tab.tabId}:${tab.syncKey}`}
        <CodeMirrorHost
          bind:this={editor}
          value={tab.draft}
          languageId={tab.language}
          documentUri={documentUri}
          readOnly={Boolean(tab.preview)}
          contentSyncKey={tab.syncKey}
          wordWrap={true}
          showFoldGutter={false}
          initialLine={tab.line}
          onFindRequested={openFindBar}
          onchange={(value) => void onDraftChanged(value)}
          onCursorChanged={(cursor) => {
            codeWorkspace.updateLine(tab.tabId, cursor.line);
          }}
        />
      {/key}
    {:else}
      <div class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <Files size={22} class="text-content-quiet" />
        <p class="text-sm text-content-secondary">Open a file from Files to sit down at the desk.</p>
      </div>
    {/if}
  </div>
</div>

{#if switcherOpen}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeFileSwitcher();
    }}
  >
    <div
      bind:this={switcherSheetEl}
      class="mobile-sheet"
      role="dialog"
      aria-label="Open files"
      tabindex="-1"
    >
      <header bind:this={switcherHeaderEl} class="mobile-sheet-stack-header">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="mobile-sheet-header-row">
          <p class="text-sm font-medium">Open files</p>
          <button
            type="button"
            class="text-sm text-content-link"
            onclick={closeFileSwitcher}
          >Close</button>
        </div>
      </header>
      <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto p-1">
        {#each tabs as openTab (openTab.tabId)}
          <button
            type="button"
            class="flex h-11 w-full items-center gap-3 rounded-lg px-3 text-left {openTab.tabId === tab?.tabId ? 'bg-surface-800 text-content-link' : 'text-content-secondary'}"
            onclick={() => void switchTo(openTab.path)}
          >
            <CodeFileIcon path={openTab.path} size={16} />
            <span class="min-w-0 flex-1 truncate font-mono text-[13px]">{openTab.title}</span>
            {#if codeWorkspace.isDirty(openTab)}
              <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-300"></span>
            {/if}
          </button>
        {:else}
          <p class="px-3 py-4 text-sm text-content-quiet">No open files.</p>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .mobile-code-editor-canvas {
    /* iOS zooms focused contenteditable text below 16px. CodeMirror measures
       before that zoom, which can separate wrapped lines from their gutters. */
    --code-editor-min-font-size: 16px;
  }
</style>
