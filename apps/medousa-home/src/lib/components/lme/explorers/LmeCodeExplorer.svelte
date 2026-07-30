<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronLeft, Code2, Download, Folder, FolderOpen, Pin, Plus, RefreshCw } from "@lucide/svelte";
  import {
    browseForgeRepositories,
    cloneProviderRepository,
    getProviderRepositoryCapabilities,
    humanPhaseLabel,
    humanizeForgeMessage,
    inspectForgeRepository,
    listForgeRepositories,
    setForgeRepositoryPinned,
    type RepositoryBrowseResponse,
    type RepositoryCatalogEntry,
    type RepositoryInspection,
    type ProviderRepositoryAdapter,
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
  let repositoryCatalog = $state<RepositoryCatalogEntry[]>([]);
  let repositoryBrowser = $state<RepositoryBrowseResponse | null>(null);
  let browserOpen = $state(false);
  let browserPurpose = $state<"repository" | "destination">("repository");
  let browserLoading = $state(false);
  let hostedOpen = $state(false);
  let hostedRepository = $state("");
  let hostedProvider = $state("");
  let hostedParent = $state("");
  let hostedAdapters = $state<ProviderRepositoryAdapter[]>([]);
  let hostedLoading = $state(false);
  let duplicateAcknowledged = $state(false);
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
  const recentRepositories = $derived(repositoryCatalog.filter((entry) => entry.available));
  const duplicateNeedsChoice = $derived(
    Boolean(repository?.existing_projects.length && !duplicateAcknowledged),
  );

  onMount(() => {
    void undertakings.refreshList();
    void loadRepositoryCatalog();
  });

  $effect(() => {
    if (!lmeWorkspace.codeCreateRequested) return;
    creating = true;
    lmeWorkspace.consumeNewCodeProjectRequest();
  });

  async function loadRepositoryCatalog() {
    try {
      repositoryCatalog = await listForgeRepositories();
      error = null;
    } catch (err) {
      repositoryCatalog = [];
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    }
  }

  async function openItem(id: string, label: string) {
    creating = false;
    await lmeWorkspace.openCodeWorkspace(id, label);
  }

  function inferredTitle(): string {
    const goal = outcome.trim().replace(/[.!?]+$/, "");
    if (goal) {
      // Outcome is the project identity (truncate only for chrome).
      return goal.length > 96 ? `${goal.slice(0, 93)}…` : goal;
    }
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
      duplicateAcknowledged = repository.existing_projects.length === 0;
      browserOpen = false;
      creating = true;
      await loadRepositoryCatalog();
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

  async function browseRepositoryFolder(
    path?: string | null,
    purpose: "repository" | "destination" = browserPurpose,
  ) {
    browserPurpose = purpose;
    browserOpen = true;
    browserLoading = true;
    error = null;
    try {
      repositoryBrowser = await browseForgeRepositories(path);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      browserLoading = false;
    }
  }

  async function openHostedRepository() {
    hostedOpen = !hostedOpen;
    if (!hostedOpen || hostedAdapters.length) return;
    try {
      hostedAdapters = (await getProviderRepositoryCapabilities()).adapters;
      hostedProvider =
        hostedAdapters.find((adapter) => adapter.available)?.provider ??
        hostedAdapters[0]?.provider ??
        "";
      if (!hostedParent && currentFolder) hostedParent = currentFolder.path;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function pickHostedParent() {
    if (!coLocated) return;
    const path = await pickExternalFolder("Choose where to keep the repository");
    if (path) hostedParent = path;
  }

  async function cloneHostedRepository() {
    if (!hostedProvider || !hostedRepository.trim() || !hostedParent.trim() || hostedLoading) return;
    hostedLoading = true;
    error = null;
    try {
      const cloned = await cloneProviderRepository({
        provider: hostedProvider,
        repository: hostedRepository.trim(),
        parent: hostedParent.trim(),
      });
      hostedRepository = "";
      hostedOpen = false;
      repository = cloned;
      repoPath = cloned.path;
      baseRef = cloned.suggested_base_ref;
      duplicateAcknowledged = true;
      browserOpen = false;
      await loadRepositoryCatalog();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      hostedLoading = false;
    }
  }

  async function togglePinned(entry: RepositoryCatalogEntry, event: MouseEvent) {
    event.stopPropagation();
    try {
      repositoryCatalog = await setForgeRepositoryPinned(entry.path, !entry.pinned);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
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
      duplicateAcknowledged = false;
      await loadRepositoryCatalog();
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
          <div class="flex items-start justify-between gap-2">
            <div class="min-w-0">
              <p class="truncate text-[11px] font-medium text-surface-200">{repository.display_name}</p>
              <p class="truncate font-mono text-[9px] text-surface-500">{repository.path}</p>
            </div>
            <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[9px] text-surface-400 hover:bg-surface-800" onclick={() => {
              repository = null;
              duplicateAcknowledged = false;
            }}>Change</button>
          </div>
          <p class="mt-1 text-[9px] leading-relaxed {repository.dirty ? 'text-amber-200' : 'text-surface-500'}">
            {repository.state_explanation}
          </p>
          <details class="mt-1 text-[9px] text-surface-600">
            <summary class="cursor-pointer select-none hover:text-surface-400">What Medousa can do here</summary>
            <p class="mt-1 leading-relaxed">{repository.trust_explanation}</p>
          </details>
        </div>
        {#if repository.existing_projects.length > 0 && !duplicateAcknowledged}
          <div class="rounded border border-primary-500/30 bg-primary-950/15 p-2">
            <p class="text-[10px] font-medium text-surface-200">You already have work here</p>
            <p class="mt-0.5 text-[9px] leading-relaxed text-surface-500">Continue it, or deliberately start a separate change.</p>
            <div class="mt-1.5 flex flex-col gap-1">
              {#each repository.existing_projects.slice(0, 3) as existing (existing.id)}
                <button type="button" class="flex items-center justify-between gap-2 rounded px-2 py-1 text-left text-[10px] text-surface-300 hover:bg-surface-800" onclick={() => void openItem(existing.id, existing.title)}>
                  <span class="min-w-0 flex-1 truncate">{existing.title}</span>
                  <span class="shrink-0 text-[8px] text-surface-500">{humanPhaseLabel(existing.human_phase)}</span>
                </button>
              {/each}
              <button type="button" class="rounded px-2 py-1 text-left text-[9px] text-primary-200 hover:bg-primary-900/25" onclick={() => (duplicateAcknowledged = true)}>Start another change</button>
            </div>
          </div>
        {/if}
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
            <button type="button" class="flex items-center gap-2 rounded border border-surface-500/30 px-2 py-2 text-left text-[10px] text-surface-200 hover:bg-surface-800" onclick={() => void browseRepositoryFolder(null, "repository")}>
              <FolderOpen size={13} class="text-primary-300" />Browse connected computer…
            </button>
          {/if}
          <button type="button" class="flex items-center gap-2 rounded px-2 py-1 text-left text-[10px] text-surface-400 hover:bg-surface-800 hover:text-surface-100" onclick={() => void openHostedRepository()}>
            <Download size={11} class="shrink-0" />
            <span>{hostedOpen ? "Hide hosted repositories" : "Clone from GitHub or GitLab…"}</span>
          </button>
          {#if hostedOpen}
            <div class="grid gap-1.5 rounded border border-surface-500/25 bg-surface-900/35 p-2">
              <div class="flex gap-1">
                {#each hostedAdapters as adapter (adapter.provider)}
                  <button
                    type="button"
                    class="rounded px-2 py-1 text-[9px] {hostedProvider === adapter.provider ? 'bg-surface-700 text-surface-100' : 'text-surface-500 hover:bg-surface-800'}"
                    class:opacity-40={!adapter.available}
                    title={adapter.message}
                    disabled={!adapter.available}
                    onclick={() => (hostedProvider = adapter.provider)}
                  >{adapter.label}</button>
                {/each}
              </div>
              <input
                class="code-field"
                aria-label="Hosted repository"
                placeholder="owner/project or repository URL"
                bind:value={hostedRepository}
              />
              <div class="flex min-w-0 items-center gap-1">
                <span class="min-w-0 flex-1 truncate font-mono text-[8px] text-surface-600">
                  {hostedParent || "Choose where to keep it"}
                </span>
                {#if coLocated}
                  <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[9px] text-surface-400 hover:bg-surface-800" onclick={() => void pickHostedParent()}>Choose…</button>
                {:else}
                  <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[9px] text-surface-400 hover:bg-surface-800" onclick={() => void browseRepositoryFolder(hostedParent || null, "destination")}>Choose…</button>
                {/if}
              </div>
              {#if hostedAdapters.length && !hostedAdapters.some((adapter) => adapter.available)}
                <p class="text-[9px] leading-relaxed text-surface-500">
                  Install and sign in to a provider CLI on the connected workshop to clone here.
                </p>
              {/if}
              <button
                type="button"
                class="rounded bg-surface-700 px-2 py-1 text-[10px] text-surface-100 disabled:opacity-40"
                disabled={hostedLoading || !hostedProvider || !hostedRepository.trim() || !hostedParent.trim()}
                onclick={() => void cloneHostedRepository()}
              >{hostedLoading ? "Cloning…" : "Clone repository"}</button>
            </div>
          {/if}
          {#each recentRepositories.slice(0, 8) as recent (recent.path)}
            <div class="group flex min-w-0 items-center rounded hover:bg-surface-800">
              <button type="button" class="flex min-w-0 flex-1 items-center gap-2 px-2 py-1 text-left text-[10px] text-surface-400 hover:text-surface-100" onclick={() => void chooseRepository(recent.path)}>
                <Code2 size={11} class="shrink-0" />
                <span class="min-w-0 flex-1 truncate">{recent.display_name}</span>
                <span class="truncate text-[8px] text-surface-600">{recent.current_branch ?? recent.suggested_base_ref}</span>
              </button>
              <button
                type="button"
                class="mr-1 shrink-0 rounded p-0.5 {recent.pinned ? 'text-primary-300' : 'text-surface-600 opacity-0 group-hover:opacity-100'}"
                aria-label={recent.pinned ? `Unpin ${recent.display_name}` : `Pin ${recent.display_name}`}
                onclick={(event) => void togglePinned(recent, event)}
              ><Pin size={10} fill={recent.pinned ? "currentColor" : "none"} /></button>
            </div>
          {/each}
          {#if !coLocated}
            <details class="px-1 text-[9px] text-surface-600">
              <summary class="cursor-pointer select-none hover:text-surface-400">Enter a path instead</summary>
              <div class="mt-1 flex gap-1">
                <input class="code-field min-w-0 flex-1" placeholder="Folder on connected computer" bind:value={repoPath} />
                <button type="button" class="rounded border border-surface-500/35 px-2 text-[10px] text-surface-300" disabled={!repoPath.trim() || inspecting} onclick={() => void chooseRepository(repoPath)}>Use</button>
              </div>
            </details>
          {/if}
        </div>
      {/if}
      {#if browserOpen && !repository}
        <div class="overflow-hidden rounded border border-surface-500/35 bg-surface-950/80">
          <div class="flex items-center gap-1 border-b border-surface-500/25 px-1.5 py-1">
            <button type="button" class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200 disabled:opacity-30" aria-label="Parent folder" disabled={!repositoryBrowser?.parent || browserLoading} onclick={() => void browseRepositoryFolder(repositoryBrowser?.parent)}><ChevronLeft size={12} /></button>
            <p class="min-w-0 flex-1 truncate font-mono text-[9px] text-surface-400">{repositoryBrowser?.path ?? "Connected computer"}</p>
            <button type="button" class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200" aria-label="Close repository browser" onclick={() => (browserOpen = false)}>×</button>
          </div>
          {#if browserLoading}
            <p class="px-2 py-3 text-[10px] text-surface-500">Looking for projects…</p>
          {:else if repositoryBrowser}
            {#if browserPurpose === "destination"}
              <button type="button" class="flex w-full items-center gap-2 border-b border-primary-500/20 bg-primary-950/15 px-2 py-1.5 text-left text-[10px] text-primary-200" onclick={() => {
                hostedParent = repositoryBrowser!.path;
                browserOpen = false;
              }}>
                <FolderOpen size={12} />Keep the repository here
              </button>
            {:else if repositoryBrowser.repository}
              <button type="button" class="flex w-full items-center gap-2 border-b border-primary-500/20 bg-primary-950/15 px-2 py-1.5 text-left text-[10px] text-primary-200" onclick={() => void chooseRepository(repositoryBrowser!.path)}>
                <Code2 size={12} />Use this repository
              </button>
            {/if}
            {#if repositoryBrowser.places.length > 1}
              <div class="flex gap-1 overflow-x-auto border-b border-surface-500/20 px-1.5 py-1">
                {#each repositoryBrowser.places as place (place.path)}
                  <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[8px] text-surface-500 hover:bg-surface-800 hover:text-surface-200" title={place.path} onclick={() => void browseRepositoryFolder(place.path)}>{place.name}</button>
                {/each}
              </div>
            {/if}
            <div class="max-h-48 overflow-y-auto py-1">
              {#if repositoryBrowser.entries.length === 0}
                <p class="px-2 py-3 text-[10px] text-surface-500">No folders here.</p>
              {:else}
                {#each repositoryBrowser.entries as entry (entry.path)}
                  <button type="button" class="flex w-full items-center gap-2 px-2 py-1 text-left text-[10px] hover:bg-surface-800" onclick={() => browserPurpose === "repository" && entry.repository ? void chooseRepository(entry.path) : void browseRepositoryFolder(entry.path, browserPurpose)}>
                    {#if entry.repository}<Code2 size={11} class="shrink-0 text-primary-300" />{:else}<Folder size={11} class="shrink-0 text-surface-500" />{/if}
                    <span class="min-w-0 flex-1 truncate {entry.repository ? 'text-surface-200' : 'text-surface-400'}">{entry.name}</span>
                    {#if entry.repository}<span class="text-[8px] text-primary-300/70">Repository</span>{/if}
                  </button>
                {/each}
              {/if}
            </div>
            {#if repositoryBrowser.truncated}
              <p class="border-t border-surface-500/20 px-2 py-1 text-[8px] text-surface-600">Showing the first 500 folders.</p>
            {/if}
          {/if}
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
        disabled={busy || inspecting || !outcome.trim() || !repository || duplicateNeedsChoice}
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
