<script lang="ts">
  import { LoaderCircle, X } from "@lucide/svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { buildTextDiff } from "$lib/diff/buildTextDiff";
  import {
    humanizeForgeMessage,
    type ForgeSourceFile,
  } from "$lib/code/codeDocumentService";
  import type { CodeWorkspaceEditPlan } from "$lib/code/codeWorkspaceEdit";
  import type { CodeDocumentTab } from "$lib/stores/codeWorkspace.svelte";
  import type { DiffFileSection } from "$lib/diff/diffTypes";

  interface Props {
    workId: string;
    comparingTabId: string | null;
    tabs: CodeDocumentTab[];
    externalVersions: Record<string, ForgeSourceFile>;
    refactorPreview: { workId: string; plan: CodeWorkspaceEditPlan } | null;
    refactorApplying: boolean;
    refactorDiffFiles: DiffFileSection[];
    refactorDiffMode: "inline" | "side";
    surfaceError: string | null;
    renameOpen: boolean;
    renameDraft: string;
    languageActionRunning: boolean;
    renameInput: HTMLInputElement | null;
    onUseProjectVersion: (tab: CodeDocumentTab) => void;
    onKeepDraft: (tab: CodeDocumentTab) => void;
    onApplyRefactor: () => void;
    onCancelRename: () => void;
    onCommitRename: () => void;
  }

  let {
    workId,
    comparingTabId = $bindable(),
    tabs,
    externalVersions,
    refactorPreview = $bindable(),
    refactorApplying,
    refactorDiffFiles,
    refactorDiffMode = $bindable(),
    surfaceError,
    renameOpen,
    renameDraft = $bindable(),
    languageActionRunning,
    renameInput = $bindable(null),
    onUseProjectVersion,
    onKeepDraft,
    onApplyRefactor,
    onCancelRename,
    onCommitRename,
  }: Props = $props();
</script>

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
          <button type="button" class="rounded px-2 py-1 text-chrome-sm text-content-secondary hover:bg-surface-800" onclick={() => onUseProjectVersion(conflictTab)}>Use project version</button>
          <button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-chrome-sm font-medium text-white" onclick={() => onKeepDraft(conflictTab)}>Keep my draft</button>
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
            onclick={() => void onApplyRefactor()}
          >{#if refactorApplying}<LoaderCircle size={11} class="animate-spin" />Applying…{:else}Apply refactor{/if}</button>
        </div>
      </footer>
    </div>
  </div>
{/if}

{#if renameOpen}
  <div class="fixed inset-0 z-[130] flex items-start justify-center px-4 pt-[18vh]">
    <button type="button" class="absolute inset-0 bg-black/35" aria-label="Cancel rename" onclick={onCancelRename}></button>
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
              void onCommitRename();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              onCancelRename();
            }
          }}
        />
      </div>
      <footer class="flex justify-end gap-2 border-t border-surface-500/30 px-3 py-2">
        <button type="button" class="rounded px-2 py-1 text-chrome-sm text-content-tertiary hover:bg-surface-800" onclick={onCancelRename}>Cancel</button>
        <button type="button" class="rounded bg-primary-500/80 px-2 py-1 text-chrome-sm font-medium text-white disabled:opacity-40" disabled={!renameDraft.trim() || languageActionRunning} onclick={() => void onCommitRename()}>Rename</button>
      </footer>
    </div>
  </div>
{/if}
