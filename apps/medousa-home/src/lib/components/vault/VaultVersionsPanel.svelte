<script lang="ts">
  import { History, RotateCcw, Save, X } from "@lucide/svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultVersions } from "$lib/stores/vaultVersions.svelte";

  let message = $state("Saved version");
  let showAdvanced = $state(false);

  const path = $derived(vault.selectedPath);

  $effect(() => {
    if (!vaultVersions.panelOpen || !vaultVersions.enabled) return;
    void vaultVersions.loadHistory(path ?? undefined);
    void vaultVersions.refresh();
  });

  async function saveVersion() {
    if (!path) return;
    const trimmed = message.trim() || "Saved version";
    await vaultVersions.saveVersion(trimmed, [path]);
  }

  async function restoreEntry(commit: string) {
    if (!path) return;
    if (!confirm("Restore this note from the selected version? Unsaved edits may be overwritten.")) {
      return;
    }
    await vaultVersions.restore(commit, path);
    await vault.openNote(path, { skipLeaveFlush: true });
  }

  async function showDiff() {
    if (!path) return;
    await vaultVersions.loadDiff(path);
  }
</script>

{#if vaultVersions.panelOpen && vaultVersions.enabled}
  <aside
    class="vault-versions-panel border-l border-surface-500/35 bg-surface-950/95"
    aria-label="Versions"
  >
    <header class="flex items-center justify-between gap-2 border-b border-surface-500/30 px-3 py-2">
      <div class="min-w-0">
        <h2 class="truncate text-sm font-semibold text-surface-50">Versions</h2>
        <p class="workshop-faint truncate text-xs">
          {#if vaultVersions.status?.isRepo}
            {vaultVersions.status.branch ?? "main"}
            ·
            {vaultVersions.status.dirtyCount === 0
              ? "up to date"
              : `${vaultVersions.status.dirtyCount} changed`}
          {:else}
            Not started yet
          {/if}
        </p>
      </div>
      <button
        type="button"
        class="rounded p-1 text-surface-300 hover:bg-surface-800 hover:text-surface-50"
        aria-label="Close versions"
        onclick={() => vaultVersions.closePanel()}
      >
        <X class="h-4 w-4" />
      </button>
    </header>

    <div class="space-y-3 p-3">
      {#if !vaultVersions.status?.isRepo}
        <p class="workshop-faint text-xs leading-snug">
          Start versioning in Settings → Versions, then save your first snapshot.
        </p>
        <button
          type="button"
          class="vault-versions-btn"
          disabled={vaultVersions.busy}
          onclick={() => void vaultVersions.startVersioning()}
        >
          Start versioning
        </button>
      {:else}
        <label class="block text-xs text-surface-300">
          Version message
          <input
            class="mt-1 w-full rounded border border-surface-500/40 bg-surface-900 px-2 py-1.5 text-sm text-surface-50"
            bind:value={message}
            disabled={vaultVersions.busy || !path}
          />
        </label>
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            class="vault-versions-btn"
            disabled={vaultVersions.busy || !path}
            onclick={() => void saveVersion()}
          >
            <Save class="h-3.5 w-3.5" />
            Save version
          </button>
          <button
            type="button"
            class="vault-versions-btn"
            disabled={vaultVersions.busy || !path}
            onclick={() => void showDiff()}
          >
            Diff vs last
          </button>
        </div>

        <div>
          <h3 class="mb-1.5 flex items-center gap-1.5 text-xs font-medium text-surface-200">
            <History class="h-3.5 w-3.5" />
            History
          </h3>
          {#if vaultVersions.history.length === 0}
            <p class="workshop-faint text-xs">No versions for this note yet.</p>
          {:else}
            <ul class="max-h-64 space-y-1.5 overflow-y-auto">
              {#each vaultVersions.history as entry (entry.id)}
                <li
                  class="rounded border border-surface-500/25 bg-surface-900/70 px-2 py-1.5"
                >
                  <p class="truncate text-sm text-surface-100">{entry.message}</p>
                  <p class="workshop-faint mt-0.5 text-[11px]">
                    {entry.shortId} · {new Date(entry.committedAt).toLocaleString()}
                  </p>
                  <button
                    type="button"
                    class="mt-1 inline-flex items-center gap-1 text-[11px] text-surface-200 underline-offset-2 hover:underline"
                    disabled={vaultVersions.busy || !path}
                    onclick={() => void restoreEntry(entry.id)}
                  >
                    <RotateCcw class="h-3 w-3" />
                    Restore
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        {#if vaultVersions.lastDiff}
          <div>
            <h3 class="mb-1 text-xs font-medium text-surface-200">Changes</h3>
            <pre
              class="max-h-40 overflow-auto rounded border border-surface-500/25 bg-surface-950 p-2 font-mono text-[10px] leading-relaxed text-surface-200"
            >{vaultVersions.lastDiff.patch || "(no differences)"}</pre>
          </div>
        {/if}

        <button
          type="button"
          class="text-[11px] text-surface-400 underline-offset-2 hover:underline"
          onclick={() => {
            showAdvanced = !showAdvanced;
            if (showAdvanced) void vaultVersions.loadWorktrees();
          }}
        >
          {showAdvanced ? "Hide Advanced Git" : "Advanced Git"}
        </button>
        {#if showAdvanced}
          <div class="rounded border border-surface-500/30 p-2 text-xs text-surface-300">
            <p>
              Branch: {vaultVersions.status.branch ?? "—"} · dirty
              {vaultVersions.status.dirtyCount}
            </p>
            <p class="mt-1 workshop-faint">Worktrees</p>
            {#if vaultVersions.worktrees.length === 0}
              <p class="workshop-faint">None listed.</p>
            {:else}
              <ul class="mt-1 space-y-1 font-mono text-[10px]">
                {#each vaultVersions.worktrees as wt (wt.path)}
                  <li class="truncate">{wt.branch ?? wt.head.slice(0, 7)} — {wt.path}</li>
                {/each}
              </ul>
            {/if}
          </div>
        {/if}
      {/if}

      {#if vaultVersions.error}
        <p class="text-xs text-rose-300/90" role="alert">{vaultVersions.error}</p>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .vault-versions-panel {
    width: min(20rem, 100%);
    flex-shrink: 0;
  }
  .vault-versions-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border-radius: 0.375rem;
    border: 1px solid color-mix(in oklab, var(--color-surface-500, #64748b) 40%, transparent);
    background: color-mix(in oklab, var(--color-surface-800, #1e293b) 80%, transparent);
    padding: 0.35rem 0.65rem;
    font-size: 0.75rem;
    color: var(--color-surface-100, #f1f5f9);
  }
  .vault-versions-btn:disabled {
    opacity: 0.5;
  }
</style>
