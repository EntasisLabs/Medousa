<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import { FileCode2, LoaderCircle, Save, X } from "@lucide/svelte";
  import type { LSPClient } from "@codemirror/lsp-client";
  import CodeMirrorHost from "$lib/components/code/CodeMirrorHost.svelte";
  import {
    acquireCodeWorkspaceLspClient,
    pathToFileUri,
  } from "$lib/code/codingEngineClient";
  import { languageSupportsLsp } from "$lib/code/codeEditorLanguageRegistry";
  import { saveUndertakingSource } from "$lib/forge";
  import {
    codeWorkspace,
    type CodeDocumentTab,
  } from "$lib/stores/codeWorkspace.svelte";

  interface Props {
    tab: CodeDocumentTab;
    worktree: string;
    leaseId?: string | null;
    generation?: number | null;
    onFocus?: () => void;
    onClose: () => void;
  }

  let { tab, worktree, leaseId = null, generation = null, onFocus, onClose }: Props = $props();
  let editor = $state<CodeMirrorHost | undefined>();
  let lspClient = $state<LSPClient | null>(null);
  let lspConnecting = $state(false);
  let lspError = $state<string | null>(null);
  let saving = $state(false);
  let linePersistTimer: ReturnType<typeof setTimeout> | null = null;

  const tabLanguage = $derived(tab.language);

  const dirty = $derived(codeWorkspace.isDirty(tab));
  const editable = $derived(Boolean(leaseId && generation != null));
  const documentUri = $derived(
    pathToFileUri(`${worktree.replace(/[\\/]$/, "")}/${tab.path}`),
  );

  async function save() {
    const liveDraft = editor?.getValue() ?? tab.draft;
    if (!leaseId || generation == null || liveDraft === tab.content || saving) return;
    if (liveDraft !== tab.draft) codeWorkspace.updateDraft(tab.tabId, liveDraft);
    editor?.flushChanges();
    saving = true;
    codeWorkspace.setError(tab.tabId, null);
    try {
      const source = await saveUndertakingSource(tab.work_id, {
        path: tab.path,
        content: liveDraft,
        lease_id: leaseId,
        generation,
        expected_digest: tab.digest,
      });
      codeWorkspace.acceptSaved(tab.tabId, source);
    } catch (err) {
      codeWorkspace.setError(
        tab.tabId,
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      saving = false;
    }
  }

  $effect(() => {
    if (!languageSupportsLsp(tabLanguage)) {
      lspClient = null;
      lspError = null;
      lspConnecting = false;
      return;
    }
    let cancelled = false;
    let release = () => {};
    lspClient = null;
    lspError = null;
    lspConnecting = true;
    const lease = acquireCodeWorkspaceLspClient({
      workId: tab.work_id,
      workspaceRoot: worktree,
      language: tabLanguage,
    });
    release = lease.release;
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
    return () => {
      cancelled = true;
      release();
    };
  });

  $effect(() => {
    const id = tab.tabId;
    const initialLine = untrack(() => tab.line);
    if (initialLine) {
      void tick().then(() => {
        if (tab.tabId === id) editor?.revealLine(initialLine);
      });
    }
  });

  onDestroy(() => {
    if (linePersistTimer) clearTimeout(linePersistTimer);
  });
</script>

<section
  class="flex min-h-64 min-w-0 flex-1 flex-col overflow-hidden border-t border-surface-500/30 bg-surface-950/25 md:border-l md:border-t-0"
  aria-label={`Secondary editor: ${tab.path}`}
  onfocusin={onFocus}
>
  <header class="flex shrink-0 items-center justify-between gap-2 border-b border-surface-500/30 px-2 py-1.5">
    <div class="flex min-w-0 items-center gap-1.5">
      <FileCode2 size={12} class="shrink-0 text-primary-300/75" />
      <div class="min-w-0">
        <p class="truncate font-mono text-[10px] text-surface-200">{tab.path}</p>
        <p class="text-[9px] text-surface-500">
          {tab.language}{dirty ? " · unsaved" : ""}{lspConnecting ? " · understanding code…" : lspError ? " · editing only" : ""}
        </p>
      </div>
    </div>
    <div class="flex shrink-0 items-center gap-0.5">
      <button
        type="button"
        class="rounded px-1.5 py-1 text-[9px] text-surface-400 hover:bg-surface-800 hover:text-surface-100 disabled:opacity-35"
        disabled={!editable || !dirty || saving}
        onclick={() => void save()}
        aria-label={`Save ${tab.title}`}
      ><Save size={10} class="sm:mr-0.5 sm:inline" /><span class="hidden sm:inline">Save</span></button>
      <button
        type="button"
        class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200"
        aria-label="Close secondary editor"
        title="Close editor group"
        onclick={onClose}
      ><X size={11} /></button>
    </div>
  </header>

  {#if tab.error}
    <p class="shrink-0 border-b border-amber-500/30 bg-amber-950/25 px-2 py-1 text-[9px] text-amber-100">{tab.error}</p>
  {/if}

  <div class="relative min-h-0 flex-1">
    {#if tab.loading}
      <div class="absolute inset-0 z-10 flex items-center justify-center bg-surface-950/70 text-xs text-surface-400">
        <LoaderCircle size={13} class="mr-2 animate-spin" />Opening file…
      </div>
    {:else if tab.digest}
      {#key tab.tabId}
        {@const editorTabId = tab.tabId}
        <CodeMirrorHost
          bind:this={editor}
          value={tab.draft}
          languageId={tab.language}
          {documentUri}
          lspLanguageId={tab.language}
          client={lspClient}
          readOnly={!editable}
          contentSyncKey={tab.syncKey}
          onchange={(value) => codeWorkspace.updateDraft(editorTabId, value)}
          onCursorChanged={(cursor) => {
            if (linePersistTimer) clearTimeout(linePersistTimer);
            linePersistTimer = setTimeout(() => {
              linePersistTimer = null;
              codeWorkspace.updateLine(editorTabId, cursor.line);
            }, 500);
          }}
        />
      {/key}
    {:else}
      <div class="flex h-full items-center justify-center p-6 text-xs text-surface-500">This file is not plain text, so Medousa cannot edit it here.</div>
    {/if}
  </div>
</section>
