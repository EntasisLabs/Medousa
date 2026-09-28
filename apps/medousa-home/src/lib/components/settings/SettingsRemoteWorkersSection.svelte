<script lang="ts">
  import { onMount } from "svelte";
  import { Cpu, Plus } from "@lucide/svelte";
  import WorkerDaemonJoinSheet from "$lib/components/workshops/WorkerDaemonJoinSheet.svelte";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import {
    listWorkerDaemons,
    removeWorkerDaemon,
    renameWorkerDaemon,
    type DaemonWorkerConnection,
  } from "$lib/utils/workerDaemonApi";

  let workers = $state<DaemonWorkerConnection[]>([]);
  let loading = $state(true);
  let busyId = $state<string | null>(null);
  let editingId = $state<string | null>(null);
  let nameDraft = $state("");
  let addOpen = $state(false);
  let error = $state<string | null>(null);

  onMount(() => {
    void refresh();
  });

  async function refresh() {
    try {
      workers = await listWorkerDaemons();
      error = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  async function refreshTargets() {
    await executionTargets.refresh({ force: true }).catch(() => undefined);
  }

  function startRename(worker: DaemonWorkerConnection) {
    editingId = worker.id;
    nameDraft = worker.label;
    error = null;
  }

  async function saveName(worker: DaemonWorkerConnection) {
    const name = nameDraft.trim();
    if (!name || name.length > 80 || busyId) return;
    busyId = worker.id;
    error = null;
    try {
      const renamed = await renameWorkerDaemon(worker.id, name);
      workers = workers.map((item) => item.id === worker.id ? renamed : item);
      editingId = null;
      await refreshTargets();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = null;
    }
  }

  async function remove(worker: DaemonWorkerConnection) {
    if (busyId || !window.confirm(`Remove ${worker.label} as a worker? Its pairing credential will be revoked.`)) return;
    busyId = worker.id;
    error = null;
    try {
      await removeWorkerDaemon(worker.id);
      workers = workers.filter((item) => item.id !== worker.id);
      if (editingId === worker.id) editingId = null;
      await refreshTargets();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busyId = null;
    }
  }

  async function paired() {
    await refresh();
    await refreshTargets();
  }
</script>

<div class="prefs-band">
  <div class="prefs-band-head">
    <div>
      <h3 class="settings-subsection-heading">Remote workers</h3>
      <p class="settings-subsection-lead">Daemons your local engine can use for delegated work.</p>
    </div>
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={() => (addOpen = true)}>
      <Plus size={15} strokeWidth={1.9} /> Add worker
    </button>
  </div>

  <div class="prefs-stack">
    {#if loading}
      <p class="workshop-faint text-sm">Loading workers…</p>
    {:else if workers.length === 0}
      <p class="workshop-faint text-sm">No remote workers yet. Add one with a full pairing link.</p>
    {:else}
      {#each workers as worker (worker.id)}
        <div class="prefs-tile">
          <Cpu size={17} strokeWidth={1.8} class="shrink-0 text-content-quiet" aria-hidden="true" />
          <span class="prefs-tile-copy min-w-0">
            {#if editingId === worker.id}
              <input
                class="input w-full text-sm"
                aria-label="Worker name"
                maxlength="80"
                bind:value={nameDraft}
                onkeydown={(event) => {
                  if (event.key === "Enter") void saveName(worker);
                  if (event.key === "Escape") editingId = null;
                }}
              />
            {:else}
              <span class="prefs-tile-title truncate">{worker.label}</span>
              <span class="prefs-tile-meta">Paired worker · {worker.workshopDeviceId.slice(0, 8)}</span>
            {/if}
          </span>
          {#if editingId === worker.id}
            <button type="button" class="prefs-tile-cta" disabled={!nameDraft.trim() || busyId !== null} onclick={() => void saveName(worker)}>Save</button>
            <button type="button" class="prefs-tile-cta" disabled={busyId !== null} onclick={() => (editingId = null)}>Cancel</button>
          {:else}
            <button type="button" class="prefs-tile-cta" disabled={busyId !== null} onclick={() => startRename(worker)}>Rename</button>
            <button type="button" class="prefs-tile-cta" disabled={busyId !== null} onclick={() => void remove(worker)}>Remove</button>
          {/if}
        </div>
      {/each}
    {/if}
    {#if error}
      <p class="text-sm text-content-error" role="alert">{error}</p>
    {/if}
  </div>
</div>

<WorkerDaemonJoinSheet open={addOpen} onClose={() => (addOpen = false)} onPaired={() => void paired()} />

<style>
  .prefs-band {
    margin-top: 1.25rem;
  }

  .prefs-band-head {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.6rem;
  }

  .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .settings-subsection-lead {
    margin-bottom: 0;
  }

  .prefs-stack {
    display: grid;
    gap: 0.5rem;
  }

  .prefs-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .prefs-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-tile-meta {
    font-size: 0.68rem;
    color: rgb(var(--theme-text-quiet));
  }

  .prefs-tile-cta {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
    cursor: pointer;
  }

  .prefs-tile-cta:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
