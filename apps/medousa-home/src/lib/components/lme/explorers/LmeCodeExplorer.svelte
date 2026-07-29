<script lang="ts">
  import { onMount } from "svelte";
  import { Code2, FolderOpen, Plus, RefreshCw } from "@lucide/svelte";
  import {
    humanPhaseLabel,
    humanizeForgeMessage,
    inspectForgeRepository,
    type RepositoryInspection,
  } from "$lib/forge";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { pickExternalFolder, rootLabelFromPath } from "$lib/utils/externalDeskApi";
  import CodeRepositoryTree from "$lib/components/lme/explorers/CodeRepositoryTree.svelte";

  let creating = $state(false);
  let busy = $state(false);
  let outcome = $state("");
  let repoPath = $state("");
  let baseRef = $state("main");
  let repository = $state<RepositoryInspection | null>(null);
  let inspecting = $state(false);
  let error = $state<string | null>(null);
  const coLocated = $derived(isCoLocatedWorkshop());
  const currentFolder = $derived(
    coLocated && vault.activeVaultRoot?.path
      ? { path: vault.activeVaultRoot.path, label: vault.activeVaultRoot.label }
      : null,
  );

  const activeItems = $derived(
    undertakings.items.filter(
      (item) =>
        item.human_phase !== "complete" &&
        item.state !== "discarded" &&
        item.state !== "accepted",
    ),
  );
  const completedItems = $derived(
    undertakings.items.filter(
      (item) =>
        item.human_phase === "complete" ||
        item.state === "discarded" ||
        item.state === "accepted",
    ),
  );
  const recentRepositories = $derived.by(() => {
    const seen = new Set<string>();
    const paths: Array<{ path: string; label: string }> = [];
    for (const item of undertakings.items) {
      const target = item.target?.Git;
      if (!target?.repo_path || seen.has(target.repo_path)) continue;
      seen.add(target.repo_path);
      paths.push({ path: target.repo_path, label: rootLabelFromPath(target.repo_path) });
      if (paths.length === 5) break;
    }
    return paths;
  });

  onMount(() => {
    void undertakings.refreshList();
  });

  async function openItem(id: string, label: string) {
    await lmeWorkspace.openCodeWorkspace(id, label);
  }

  function inferredTitle(): string {
    const goal = outcome.trim().replace(/[.!?]+$/, "");
    if (goal) return goal.length > 72 ? `${goal.slice(0, 69)}…` : goal;
    return repository?.display_name ?? rootLabelFromPath(repoPath);
  }

  async function chooseRepository(path: string) {
    if (!path.trim() || inspecting) return;
    inspecting = true;
    error = null;
    try {
      repository = await inspectForgeRepository(path.trim());
      repoPath = repository.path;
      baseRef = repository.suggested_base_ref;
      creating = true;
    } catch (err) {
      repository = null;
      repoPath = path.trim();
      error = err instanceof Error ? err.message : String(err);
    } finally {
      inspecting = false;
    }
  }

  async function pickRepository() {
    if (!coLocated) return;
    const path = await pickExternalFolder("Choose a code project");
    if (path) await chooseRepository(path);
  }

  async function create() {
    if (!outcome.trim() || !repository || busy) return;
    busy = true;
    error = null;
    try {
      const item = await undertakings.start({
        title: inferredTitle(),
        brief: outcome.trim(),
        repo_path: repository.path,
        base_ref: baseRef.trim() || "main",
      });
      creating = false;
      outcome = "";
      repository = null;
      repoPath = "";
      await lmeWorkspace.openCodeWorkspace(item.id, item.title);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<aside class="flex h-full min-h-0 w-full flex-col" aria-label="Code projects">
  <div class="flex shrink-0 items-center justify-between border-b border-surface-500/30 px-2 py-1.5">
    <span class="flex items-center gap-1.5 text-[11px] font-medium text-surface-300">
      <Code2 size={13} strokeWidth={1.8} />
      Projects
    </span>
    <div class="flex items-center gap-0.5">
      <button
        type="button"
        class="rounded p-1 text-surface-400 hover:bg-surface-800 hover:text-surface-100"
        aria-label="Refresh projects"
        title="Refresh"
        onclick={() => void undertakings.refreshList()}
      ><RefreshCw size={13} /></button>
      <button
        type="button"
        class="rounded p-1 text-surface-400 hover:bg-surface-800 hover:text-surface-100"
        aria-label="New code project"
        title="New code project"
      onclick={() => (creating = !creating)}
      ><Plus size={14} /></button>
    </div>
  </div>

  {#if creating}
    <form
      class="flex shrink-0 flex-col gap-1.5 border-b border-surface-500/30 p-2"
      onsubmit={(event) => {
        event.preventDefault();
        void create();
      }}
    >
      {#if repository}
        <div class="rounded border border-surface-500/30 bg-surface-900/45 px-2 py-1.5">
          <p class="truncate text-[11px] font-medium text-surface-200">{repository.display_name}</p>
          <p class="truncate font-mono text-[9px] text-surface-500">{repository.path}</p>
          {#if repository.dirty}
            <p class="mt-1 text-[9px] leading-relaxed text-amber-200">
              {repository.changed_files} uncommitted {repository.changed_files === 1 ? "file is" : "files are"} outside this project and won’t be included.
            </p>
          {/if}
        </div>
      {:else}
        <div class="grid gap-1">
          {#if coLocated}
            <button type="button" class="flex items-center gap-2 rounded border border-surface-500/30 px-2 py-2 text-left text-[10px] text-surface-200 hover:bg-surface-800" onclick={() => void pickRepository()}>
              <FolderOpen size={13} class="text-primary-300" />Choose a folder…
            </button>
            {#if currentFolder}
              <button type="button" class="flex min-w-0 items-center gap-2 rounded px-2 py-1 text-left text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100" onclick={() => void chooseRepository(currentFolder.path)}>
                <FolderOpen size={11} class="shrink-0" /><span class="min-w-0 flex-1 truncate">Current folder · {currentFolder.label}</span>
              </button>
            {/if}
          {:else}
            <div class="flex gap-1">
              <input class="code-field min-w-0 flex-1" placeholder="Folder on connected computer" bind:value={repoPath} />
              <button type="button" class="rounded border border-surface-500/35 px-2 text-[10px] text-surface-300" disabled={!repoPath.trim() || inspecting} onclick={() => void chooseRepository(repoPath)}>Use</button>
            </div>
          {/if}
          {#each recentRepositories as recent (recent.path)}
            <button type="button" class="flex min-w-0 items-center gap-2 rounded px-2 py-1 text-left text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100" onclick={() => void chooseRepository(recent.path)}>
              <Code2 size={11} class="shrink-0" /><span class="min-w-0 flex-1 truncate">{recent.label}</span>
            </button>
          {/each}
        </div>
      {/if}
      <label class="code-field-label">
        <span>What do you want to accomplish?</span>
        <textarea class="code-field min-h-16 resize-none" placeholder="Make indexing cancellation-safe" bind:value={outcome}></textarea>
      </label>
      {#if repository}
        <details class="text-[9px] text-surface-500">
          <summary class="cursor-pointer select-none hover:text-surface-300">Starting point</summary>
          <label class="mt-1 flex items-center gap-2">
            <span class="shrink-0">Branch</span>
            <input class="code-field min-w-0 flex-1" bind:value={baseRef} />
          </label>
        </details>
      {/if}
      <button
        type="submit"
        class="rounded bg-primary-500/80 px-2 py-1 text-xs font-medium text-surface-50 disabled:opacity-40"
        disabled={busy || inspecting || !outcome.trim() || !repository}
      >{busy ? "Preparing project…" : inspecting ? "Reading project…" : "Start"}</button>
    </form>
  {/if}

  {#if error || undertakings.error}
    <p class="m-2 rounded border border-amber-500/35 bg-amber-950/25 px-2 py-1 text-[10px] text-amber-100">
      {humanizeForgeMessage(error || undertakings.error || "")}
    </p>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto py-1.5">
    {#if undertakings.loading && undertakings.items.length === 0}
      <p class="px-3 py-3 text-xs text-surface-500">Loading projects…</p>
    {:else if undertakings.items.length === 0}
      <div class="px-3 py-5 text-center">
        <Code2 size={20} class="mx-auto text-surface-600" />
        <p class="mt-2 text-xs text-surface-400">No code projects yet.</p>
        <p class="mt-1 text-[10px] leading-relaxed text-surface-500">
          Start with a repository and the change you want to make. Medousa will keep the work together.
        </p>
      </div>
    {/if}

    {#if activeItems.length}
      <p class="px-3 pb-1 pt-1 text-[9px] font-medium uppercase tracking-wider text-surface-500">In progress</p>
      {#each activeItems as item (item.id)}
        <button
          type="button"
          class="w-full px-3 py-2 text-left transition hover:bg-surface-800/70 {undertakings.selectedId === item.id ? 'bg-surface-800' : ''}"
          onclick={() => void openItem(item.id, item.title)}
        >
          <span class="block truncate text-xs font-medium text-surface-100">{item.title}</span>
          <span class="mt-0.5 block truncate text-[9px] text-surface-500">
            {humanPhaseLabel(item.human_phase)}
          </span>
        </button>
        {#if undertakings.selectedId === item.id}
          <CodeRepositoryTree workId={item.id} prepared={Boolean(item.environment)} />
        {/if}
      {/each}
    {/if}

    {#if completedItems.length}
      <p class="px-3 pb-1 pt-3 text-[9px] font-medium uppercase tracking-wider text-surface-500">Finished</p>
      {#each completedItems as item (item.id)}
        <button
          type="button"
          class="w-full px-3 py-2 text-left opacity-65 transition hover:bg-surface-800/70 hover:opacity-100 {undertakings.selectedId === item.id ? 'bg-surface-800 opacity-100' : ''}"
          onclick={() => void openItem(item.id, item.title)}
        >
          <span class="block truncate text-xs font-medium text-surface-200">{item.title}</span>
          <span class="mt-0.5 block truncate text-[9px] text-surface-500">
            {humanPhaseLabel(item.human_phase)}
          </span>
        </button>
        {#if undertakings.selectedId === item.id}
          <CodeRepositoryTree workId={item.id} prepared={Boolean(item.environment)} />
        {/if}
      {/each}
    {/if}
  </div>
</aside>

<style>
  .code-field {
    width: 100%;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    border-radius: 0.25rem;
    background: rgb(var(--color-surface-900));
    padding: 0.3rem 0.45rem;
    font-size: 0.7rem;
  }

  .code-field-label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    color: rgb(var(--color-surface-500));
    font-size: 0.58rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
</style>
