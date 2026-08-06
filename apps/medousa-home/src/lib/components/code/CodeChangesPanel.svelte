<script lang="ts">
  import type { ForgeChanges } from "$lib/forge";

  type Props = {
    changes: ForgeChanges | null;
    loading?: boolean;
    error?: string | null;
    onOpenPath: (path: string) => void;
    onClose: () => void;
    onRefresh: () => void;
  };

  let {
    changes,
    loading = false,
    error = null,
    onOpenPath,
    onClose,
    onRefresh,
  }: Props = $props();

  function statusLabel(status: string): string {
    switch (status) {
      case "added":
        return "A";
      case "modified":
        return "M";
      case "deleted":
        return "D";
      case "renamed":
        return "R";
      case "copied":
        return "C";
      case "type_changed":
        return "T";
      case "untracked":
        return "U";
      case "unmerged":
        return "!";
      default:
        return "?";
    }
  }

  function trackingLine(snapshot: ForgeChanges): string {
    const parts: string[] = [];
    if (snapshot.detached) {
      parts.push("detached HEAD");
    } else if (snapshot.branch) {
      parts.push(snapshot.branch);
    }
    if (snapshot.upstream) {
      const ahead = snapshot.ahead ?? 0;
      const behind = snapshot.behind ?? 0;
      if (ahead === 0 && behind === 0) {
        parts.push(`↑ ${snapshot.upstream}`);
      } else {
        parts.push(`↑ ${snapshot.upstream} +${ahead} −${behind}`);
      }
    }
    if (snapshot.base_ref) {
      parts.push(`base ${snapshot.base_ref}`);
    }
    return parts.join(" · ") || "Working copy";
  }
</script>

<div class="max-h-52 shrink-0 overflow-y-auto border-t border-surface-500/25 bg-surface-950/90">
  <div class="sticky top-0 flex items-center justify-between gap-2 bg-surface-950 px-2.5 py-1 text-[9px] uppercase tracking-wider text-content-quiet">
    <span>Changes</span>
    <div class="flex items-center gap-1 normal-case tracking-normal">
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[9px] text-content-quiet hover:bg-surface-800 hover:text-content-secondary"
        onclick={onRefresh}
      >Refresh</button>
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[9px] text-content-quiet hover:bg-surface-800 hover:text-content-secondary"
        onclick={onClose}
      >Hide</button>
    </div>
  </div>
  {#if error}
    <p class="px-2.5 py-2 text-[10px] text-rose-200/90">{error}</p>
  {:else if loading && !changes}
    <p class="px-2.5 py-2 text-[10px] text-content-quiet">Loading changes…</p>
  {:else if changes}
    <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 border-b border-surface-500/15 px-2.5 py-1.5 text-[10px] text-content-secondary">
      <span class="font-mono text-content-tertiary">{trackingLine(changes)}</span>
      {#if changes.conflict}
        <span class="rounded bg-rose-950/50 px-1.5 py-0.5 text-[9px] text-rose-200">Conflict</span>
      {/if}
      <span class="text-content-quiet">{changes.files.length} file{changes.files.length === 1 ? "" : "s"}</span>
    </div>
    {#if changes.files.length === 0}
      <p class="px-2.5 py-2 text-[10px] text-content-quiet">Working tree is clean.</p>
    {:else}
      {#each changes.files as file (file.path)}
        <button
          type="button"
          class="flex w-full items-center gap-2 border-b border-surface-500/10 px-2.5 py-1 text-left text-[10px] text-content-secondary hover:bg-surface-800/60 {file.status === 'unmerged' ? 'text-rose-200/90' : ''}"
          onclick={() => onOpenPath(file.path)}
        >
          <span class="w-3 shrink-0 font-mono text-[9px] text-content-quiet" title={file.status}>{statusLabel(file.status)}</span>
          <span class="min-w-0 flex-1 truncate font-mono">{file.path}{#if file.old_path}<span class="text-content-faint"> ← {file.old_path}</span>{/if}</span>
        </button>
      {/each}
    {/if}
  {:else}
    <p class="px-2.5 py-2 text-[10px] text-content-quiet">Open Changes to load branch and file status.</p>
  {/if}
</div>
