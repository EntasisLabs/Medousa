<script lang="ts">
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import CodeFileIcon from "$lib/components/lme/explorers/CodeFileIcon.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import {
    getChangesFile,
    getForgeChanges,
    humanizeForgeMessage,
    type ChangesFileDiff,
    type ForgeChanges,
  } from "$lib/forge";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";
  import { openMobileCodeFile } from "$lib/utils/mobileCodeOpen";

  interface Props {
    workId: string;
  }

  let { workId }: Props = $props();

  let snapshot = $state<ForgeChanges | null>(null);
  let fileDiff = $state<ChangesFileDiff | null>(null);
  let loading = $state(false);
  let fileLoading = $state(false);
  let error = $state<string | null>(null);

  const selectedPath = $derived(mobileCodeWorkspaceState.presentation?.changesPath ?? null);

  $effect(() => {
    void workId;
    void refresh();
  });

  $effect(() => {
    if (!selectedPath) return;
    return registerMobileBackHandler(() => {
      mobileCodeWorkspaceState.setChangesPath(null);
      fileDiff = null;
      return true;
    });
  });

  async function refresh() {
    loading = true;
    error = null;
    try {
      snapshot = await getForgeChanges(workId);
    } catch (err) {
      snapshot = null;
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      loading = false;
    }
  }

  async function openDiff(path: string) {
    haptic("light");
    mobileCodeWorkspaceState.setChangesPath(path);
    fileLoading = true;
    try {
      fileDiff = await getChangesFile(workId, path);
    } catch (err) {
      fileDiff = null;
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      fileLoading = false;
    }
  }

  function toStackFile(diff: ChangesFileDiff): DiffFileSection {
    const stats = countDiffStats(diff.hunks);
    return {
      path: diff.path,
      oldPath: diff.old_path,
      status: diff.status,
      binary: diff.binary,
      conflict: diff.conflict,
      additions: stats.additions,
      deletions: stats.deletions,
      hunks: diff.hunks,
      baselineBytes: diff.baseline.byte_size,
      reviewedBytes: diff.working.byte_size,
      baselineExists: diff.baseline.exists,
      reviewedExists: diff.working.exists,
      beforeText: diff.baseline.content ?? null,
      afterText: diff.working.content ?? null,
    };
  }

  const stackFiles = $derived(fileDiff ? [toStackFile(fileDiff)] : []);
</script>

<div class="flex h-full min-h-0 flex-col">
  {#if selectedPath && fileDiff}
    <div class="min-h-0 flex-1 overflow-hidden">
      <DiffStack
        files={stackFiles}
        chrome="none"
        density="compact"
        mode="inline"
        onOpenFile={(path, line) => {
          void openMobileCodeFile(workId, path, { line, origin: "changes" });
        }}
      />
    </div>
  {:else}
    {#if error}
      <p class="m-3 rounded border border-amber-500/35 bg-amber-950/25 px-3 py-2 text-[12px] text-amber-100">
        {error}
      </p>
    {/if}
    <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto">
      {#if loading && !snapshot}
        <p class="px-4 py-6 text-sm text-content-quiet">Loading changes…</p>
      {:else if !snapshot?.files.length}
        <p class="px-4 py-6 text-sm text-content-quiet">No changed files in this working copy.</p>
      {:else}
        {#if snapshot.branch || snapshot.dirty}
          <p class="px-4 py-2 text-[11px] text-content-quiet">
            {[snapshot.branch, snapshot.dirty ? "unsealed edits" : null].filter(Boolean).join(" · ")}
          </p>
        {/if}
        {#each snapshot.files as file (file.path)}
          <button
            type="button"
            class="flex min-h-11 w-full items-center gap-3 px-3 text-left active:bg-surface-800"
            onclick={() => void openDiff(file.path)}
          >
            <CodeFileIcon path={file.path} size={16} />
            <span class="min-w-0 flex-1 truncate text-sm text-content-secondary">{file.path}</span>
            <span class="shrink-0 font-mono text-[10px] text-amber-200/90">{file.status}</span>
          </button>
        {/each}
      {/if}
      {#if fileLoading}
        <p class="px-4 py-3 text-[12px] text-content-quiet">Opening diff…</p>
      {/if}
    </div>
  {/if}
</div>
