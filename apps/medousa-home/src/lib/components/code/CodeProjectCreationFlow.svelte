<script lang="ts">
  import { onMount } from "svelte";
  import {
    ArrowLeft,
    ArrowUp,
    ChevronRight,
    CircleDot,
    Cloud,
    Code2,
    Download,
    Folder,
    FolderOpen,
    GitBranch,
    Laptop,
    Pin,
    Plus,
    X,
  } from "@lucide/svelte";
  import {
    browseForgeRepositories,
    cloneProviderRepository,
    getProviderRepositoryCapabilities,
    getUndertaking,
    humanPhaseLabel,
    humanizeForgeMessage,
    inspectForgeRepository,
    listForgeRepositories,
    setForgeRepositoryPinned,
    type ItemProjection,
    type ProviderRepositoryAdapter,
    type RepositoryBrowseResponse,
    type RepositoryCatalogEntry,
    type RepositoryInspection,
  } from "$lib/forge";
  import { setSessionCodeBinding } from "$lib/daemon";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { pickExternalFolder, rootLabelFromPath } from "$lib/utils/externalDeskApi";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";

  interface Props {
    presentation?: "rail" | "popover";
    sessionId?: string | null;
    initialRepositoryPath?: string | null;
    onCancel?: () => void;
    onCreated?: (item: ItemProjection) => void | Promise<void>;
    onContinue?: (item: ItemProjection) => void | Promise<void>;
    onCatalogChanged?: () => void | Promise<void>;
  }

  let {
    presentation = "rail",
    sessionId = null,
    initialRepositoryPath = null,
    onCancel,
    onCreated,
    onContinue,
    onCatalogChanged,
  }: Props = $props();

  let busy = $state(false);
  let inspecting = $state(false);
  let error = $state<string | null>(null);
  let outcome = $state("");
  let repoPath = $state("");
  let baseRef = $state("");
  let repository = $state<RepositoryInspection | null>(null);
  let repositoryCatalog = $state<RepositoryCatalogEntry[]>([]);
  let duplicateAcknowledged = $state(false);
  let browser = $state<RepositoryBrowseResponse | null>(null);
  let browserOpen = $state(false);
  let browserLoading = $state(false);
  let browserPurpose = $state<"repository" | "destination">("repository");
  let branchOpen = $state(false);
  let branchStep = $state<"source" | "local" | "remotes" | "remote" | "revision">("source");
  let selectedRemote = $state<string | null>(null);
  let branchQuery = $state("");
  let manualRevision = $state("");
  let hostedOpen = $state(false);
  let hostedLoading = $state(false);
  let hostedRepository = $state("");
  let hostedProvider = $state("");
  let hostedParent = $state("");
  let hostedAdapters = $state<ProviderRepositoryAdapter[]>([]);

  const coLocated = $derived(isCoLocatedWorkshop());
  const currentFolder = $derived(
    coLocated && vault.activeVaultRoot?.path
      ? { path: vault.activeVaultRoot.path, label: vault.activeVaultRoot.label }
      : null,
  );
  const recentRepositories = $derived(repositoryCatalog.filter((entry) => entry.available));
  const localBranches = $derived.by(() => {
    const branches = repository?.local_branches?.filter(Boolean) ?? [];
    if (branches.length > 0) return branches;
    return [...new Set(
      [repository?.current_branch, repository?.suggested_base_ref].filter(
        (value): value is string => Boolean(value?.trim()),
      ),
    )];
  });
  const remoteGroups = $derived(repository?.remote_branches ?? []);
  const selectedRemoteGroup = $derived(
    remoteGroups.find((remote) => remote.name === selectedRemote) ?? null,
  );
  const branchLabel = $derived.by(() => {
    for (const remote of remoteGroups) {
      const prefix = `${remote.name}/`;
      if (baseRef.startsWith(prefix)) return `Remote / ${remote.name} / ${baseRef.slice(prefix.length)}`;
    }
    return `Local / ${baseRef || "Choose branch"}`;
  });
  const ready = $derived(
    Boolean(repository && repository.has_commits !== false && baseRef.trim() && outcome.trim()),
  );

  onMount(() => {
    void loadCatalog();
    if (initialRepositoryPath?.trim()) void chooseRepository(initialRepositoryPath);
  });

  async function loadCatalog() {
    try {
      repositoryCatalog = await listForgeRepositories();
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    }
  }

  async function chooseRepository(path: string) {
    if (!path.trim() || inspecting) return;
    inspecting = true;
    error = null;
    try {
      repository = await inspectForgeRepository(path.trim());
      repoPath = repository.path;
      baseRef = repository.suggested_base_ref ?? repository.current_branch ?? "";
      duplicateAcknowledged = repository.existing_projects.length === 0;
      branchOpen = false;
      browserOpen = false;
      await loadCatalog();
    } catch (err) {
      repository = null;
      repoPath = path.trim();
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      inspecting = false;
    }
  }

  async function pickRepository() {
    if (!coLocated) return;
    const path = await pickExternalFolder("Choose a code project");
    if (path) await chooseRepository(path);
  }

  async function browse(path?: string | null, purpose = browserPurpose) {
    browserPurpose = purpose;
    browserOpen = true;
    browserLoading = true;
    error = null;
    try {
      browser = await browseForgeRepositories(path);
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      browserLoading = false;
    }
  }

  async function toggleHosted() {
    hostedOpen = !hostedOpen;
    if (!hostedOpen || hostedAdapters.length > 0) return;
    try {
      hostedAdapters = (await getProviderRepositoryCapabilities()).adapters;
      hostedProvider = hostedAdapters.find((adapter) => adapter.available)?.provider ?? "";
      hostedParent = currentFolder?.path ?? "";
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    }
  }

  async function chooseHostedParent() {
    if (coLocated) {
      const path = await pickExternalFolder("Choose where to keep the repository");
      if (path) hostedParent = path;
      return;
    }
    await browse(hostedParent || null, "destination");
  }

  async function cloneHosted() {
    if (hostedLoading || !hostedProvider || !hostedRepository.trim() || !hostedParent.trim()) return;
    hostedLoading = true;
    error = null;
    try {
      const cloned = await cloneProviderRepository({
        provider: hostedProvider,
        repository: hostedRepository.trim(),
        parent: hostedParent.trim(),
      });
      hostedOpen = false;
      hostedRepository = "";
      repository = cloned;
      repoPath = cloned.path;
      baseRef = cloned.suggested_base_ref ?? cloned.current_branch ?? "";
      duplicateAcknowledged = true;
      await loadCatalog();
      await onCatalogChanged?.();
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      hostedLoading = false;
    }
  }

  async function togglePinned(entry: RepositoryCatalogEntry, event: MouseEvent) {
    event.stopPropagation();
    try {
      repositoryCatalog = await setForgeRepositoryPinned(entry.path, !entry.pinned);
      await onCatalogChanged?.();
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    }
  }

  function chooseBranch(reference: string) {
    baseRef = reference;
    branchOpen = false;
    branchQuery = "";
  }

  function filtered(branches: string[]) {
    const query = branchQuery.trim().toLowerCase();
    return query ? branches.filter((branch) => branch.toLowerCase().includes(query)) : branches;
  }

  async function bindItem(item: ItemProjection) {
    const id = sessionId?.trim();
    if (!id) return;
    await setSessionCodeBinding(id, item.id);
    undertakings.setActiveFromItem(item);
    undertakings.bindChat(id);
    window.dispatchEvent(new CustomEvent("medousa-code-project-binding-changed", {
      detail: { sessionId: id, workId: item.id },
    }));
  }

  async function continueItem(existing: { id: string }) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      const item = await getUndertaking(existing.id);
      await bindItem(item);
      await onContinue?.(item);
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function create() {
    if (!ready || !repository || busy) return;
    busy = true;
    error = null;
    try {
      const title = outcome.trim().replace(/[.!?]+$/, "");
      const item = await undertakings.start({
        title: title.length > 96 ? `${title.slice(0, 93)}…` : title || rootLabelFromPath(repoPath),
        brief: outcome.trim(),
        repo_path: repository.path,
        base_ref: baseRef.trim(),
      });
      await bindItem(item);
      await loadCatalog();
      await onCatalogChanged?.();
      await onCreated?.(item);
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }
</script>

<form
  class="code-project-creation"
  class:code-project-creation--popover={presentation === "popover"}
  onsubmit={(event) => { event.preventDefault(); void create(); }}
>
  <header class="creation-header">
    <div class="min-w-0">
      <p class="text-sm font-medium text-surface-100">New change</p>
      <p class="truncate text-xs text-content-quiet">
        {repository ? `In ${repository.display_name}` : "Choose a repository to begin"}
      </p>
    </div>
    {#if onCancel}
      <button type="button" class="creation-icon" aria-label="Cancel new change" onclick={onCancel}>
        <X size={14} strokeWidth={1.75} />
      </button>
    {/if}
  </header>

  <div class="creation-body">
    {#if repository}
      <section class="creation-card">
        <div class="flex min-w-0 items-start gap-2.5">
          <span class="creation-repo-icon"><Code2 size={15} strokeWidth={1.75} /></span>
          <div class="min-w-0 flex-1">
            <p class="truncate text-sm font-medium text-surface-100">{repository.display_name}</p>
            <p class="mt-0.5 truncate font-mono text-xs text-content-quiet">{repository.path}</p>
            {#if repository.dirty}
              <p class="creation-status" title={repository.state_explanation}>
                <CircleDot size={9} />{repository.changed_files} local {repository.changed_files === 1 ? "change" : "changes"}
              </p>
            {/if}
          </div>
          <button type="button" class="creation-link" onclick={() => {
            repository = null;
            duplicateAcknowledged = false;
          }}>Change</button>
        </div>
      </section>

      {#if repository.existing_projects.length > 0 && !duplicateAcknowledged}
        <section class="mt-3">
          <p class="px-1 text-xs font-medium text-surface-200">Active changes</p>
          <p class="px-1 text-xs text-content-quiet">Continue one, or start a separate working copy.</p>
          <div class="creation-list mt-1.5">
            {#each repository.existing_projects.slice(0, 5) as existing (existing.id)}
              <button type="button" class="creation-row" disabled={busy} onclick={() => void continueItem(existing)}>
                <CircleDot size={12} class="text-primary-300" />
                <span class="min-w-0 flex-1 truncate">{existing.title}</span>
                <span class="text-content-quiet">{humanPhaseLabel(existing.human_phase)}</span>
              </button>
            {/each}
          </div>
          <button type="button" class="creation-separate" onclick={() => (duplicateAcknowledged = true)}>
            <Plus size={13} />Start another change
          </button>
        </section>
      {:else}
        <section class="mt-3">
          <label for="project-change-outcome" class="px-1 text-xs font-medium text-surface-200">What do you want changed?</label>
          <div class="creation-prompt mt-1.5">
            <textarea
              id="project-change-outcome"
              class="min-h-24 w-full resize-none border-0 bg-transparent text-sm text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
              placeholder="Make indexing cancellation-safe…"
              bind:value={outcome}
              onkeydown={(event) => {
                if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                  event.preventDefault();
                  void create();
                }
              }}
            ></textarea>
            <div class="creation-toolbar">
              <button type="button" class="creation-branch-trigger" onclick={() => {
                branchOpen = !branchOpen;
                branchStep = "source";
              }}>
                <GitBranch size={12} /><span class="truncate">{branchLabel}</span><ChevronRight size={11} />
              </button>
              <button type="submit" class="creation-submit" disabled={busy || inspecting || !ready} aria-label="Start change">
                <ArrowUp size={15} strokeWidth={2} />
              </button>
            </div>
          </div>

          {#if branchOpen}
            <div class="creation-branch-panel">
              {#if branchStep !== "source"}
                <button type="button" class="creation-back" onclick={() => {
                  branchStep = branchStep === "remote" && remoteGroups.length > 1 ? "remotes" : "source";
                  branchQuery = "";
                }}><ArrowLeft size={12} />Back</button>
              {/if}
              {#if branchStep === "source"}
                <button type="button" class="creation-source" onclick={() => (branchStep = "local")}>
                  <Laptop size={14} /><span class="flex-1">Local</span><ChevronRight size={12} />
                </button>
                <button type="button" class="creation-source" disabled={remoteGroups.length === 0} onclick={() => {
                  if (remoteGroups.length === 1) {
                    selectedRemote = remoteGroups[0].name;
                    branchStep = "remote";
                  } else branchStep = "remotes";
                }}><Cloud size={14} /><span class="flex-1">Remote</span><ChevronRight size={12} /></button>
                <button type="button" class="creation-link px-2 py-1.5" onclick={() => {
                  branchStep = "revision";
                  manualRevision = baseRef;
                }}>Enter a branch or commit…</button>
              {:else if branchStep === "remotes"}
                {#each remoteGroups as remote (remote.name)}
                  <button type="button" class="creation-source" onclick={() => {
                    selectedRemote = remote.name;
                    branchStep = "remote";
                  }}><Cloud size={13} /><span class="flex-1">{remote.name}</span><span class="text-content-quiet">{remote.branches.length}</span><ChevronRight size={11} /></button>
                {/each}
              {:else if branchStep === "local" || branchStep === "remote"}
                <input class="creation-field" placeholder="Filter branches" bind:value={branchQuery} />
                <div class="max-h-40 overflow-y-auto">
                  {#each filtered(branchStep === "local" ? localBranches : selectedRemoteGroup?.branches ?? []) as branch (branch)}
                    <button type="button" class="creation-row" onclick={() => chooseBranch(branchStep === "remote" ? `${selectedRemote}/${branch}` : branch)}>
                      <GitBranch size={12} /><span class="truncate">{branch}</span>
                    </button>
                  {/each}
                </div>
              {:else}
                <div class="flex gap-1.5">
                  <input class="creation-field min-w-0 flex-1" placeholder="branch, tag, or commit" bind:value={manualRevision} />
                  <button type="button" class="creation-use" disabled={!manualRevision.trim()} onclick={() => chooseBranch(manualRevision.trim())}>Use</button>
                </div>
              {/if}
            </div>
          {/if}
        </section>
      {/if}
    {:else}
      <p class="px-1 pb-1 text-xs font-medium text-content-tertiary">Open repository</p>
      {#if coLocated}
        <button type="button" class="creation-source-card" onclick={() => void pickRepository()}>
          <FolderOpen size={15} class="text-content-link" />
          <span class="min-w-0 flex-1 text-left"><span class="block text-sm font-medium">Choose a folder…</span><span class="text-xs text-content-quiet">Open a repository on this computer</span></span>
        </button>
        {#if currentFolder}
          <button type="button" class="creation-row" onclick={() => void chooseRepository(currentFolder.path)}>
            <FolderOpen size={13} /><span class="truncate">Current folder · {currentFolder.label}</span>
          </button>
        {/if}
      {:else}
        <button type="button" class="creation-source-card" onclick={() => void browse(null, "repository")}>
          <FolderOpen size={15} class="text-content-link" />
          <span class="min-w-0 flex-1 text-left"><span class="block text-sm font-medium">Browse connected computer…</span><span class="text-xs text-content-quiet">Open a repository on the workshop</span></span>
        </button>
      {/if}

      <button type="button" class="creation-row" onclick={() => void toggleHosted()}>
        <Download size={13} /><span>{hostedOpen ? "Hide hosted repositories" : "Clone from GitHub or GitLab…"}</span>
      </button>
      {#if hostedOpen}
        <div class="creation-subpanel">
          <div class="flex flex-wrap gap-1">
            {#each hostedAdapters as adapter (adapter.provider)}
              <button type="button" class="creation-provider" class:creation-provider--active={hostedProvider === adapter.provider} disabled={!adapter.available} onclick={() => (hostedProvider = adapter.provider)}>{adapter.label}</button>
            {/each}
          </div>
          <input class="creation-field" placeholder="owner/project or repository URL" bind:value={hostedRepository} />
          <button type="button" class="creation-row" onclick={() => void chooseHostedParent()}>
            <Folder size={12} /><span class="truncate">{hostedParent || "Choose destination…"}</span>
          </button>
          <button type="button" class="creation-use" disabled={hostedLoading || !hostedProvider || !hostedRepository.trim() || !hostedParent.trim()} onclick={() => void cloneHosted()}>{hostedLoading ? "Cloning…" : "Clone repository"}</button>
        </div>
      {/if}

      {#if recentRepositories.length > 0}
        <p class="px-1 pb-1 pt-3 text-xs font-medium text-content-tertiary">Recent</p>
        <div class="creation-list">
          {#each recentRepositories.slice(0, presentation === "popover" ? 5 : 8) as recent (recent.path)}
            <div class="creation-recent group">
              <button type="button" class="flex min-w-0 flex-1 items-center gap-2 text-left" onclick={() => void chooseRepository(recent.path)}>
                <Code2 size={13} /><span class="min-w-0 flex-1"><span class="block truncate text-xs text-surface-200">{recent.display_name}</span><span class="block truncate text-[10px] text-content-quiet">{recent.current_branch ?? recent.suggested_base_ref ?? "Repository"}</span></span>
              </button>
              <button type="button" class="creation-pin" aria-label={recent.pinned ? `Unpin ${recent.display_name}` : `Pin ${recent.display_name}`} onclick={(event) => void togglePinned(recent, event)}><Pin size={12} fill={recent.pinned ? "currentColor" : "none"} /></button>
            </div>
          {/each}
        </div>
      {/if}

      {#if !coLocated}
        <details class="mt-2 text-[10px] text-content-faint">
          <summary>Enter a path instead</summary>
          <div class="mt-1 flex gap-1"><input class="creation-field min-w-0 flex-1" bind:value={repoPath} /><button type="button" class="creation-use" onclick={() => void chooseRepository(repoPath)}>Use</button></div>
        </details>
      {/if}

      {#if browserOpen}
        <div class="creation-browser">
          <div class="creation-browser-head">
            <button type="button" class="creation-icon" disabled={!browser?.parent || browserLoading} onclick={() => void browse(browser?.parent)}><ArrowLeft size={12} /></button>
            <span class="min-w-0 flex-1 truncate font-mono text-[10px] text-content-tertiary">{browser?.path ?? "Connected computer"}</span>
            <button type="button" class="creation-icon" onclick={() => (browserOpen = false)}><X size={12} /></button>
          </div>
          {#if browserLoading}<p class="p-3 text-xs text-content-quiet">Looking for projects…</p>{:else if browser}
            {#if browserPurpose === "destination"}
              <button type="button" class="creation-row creation-row--primary" onclick={() => { hostedParent = browser!.path; browserOpen = false; }}><FolderOpen size={12} />Keep the repository here</button>
            {:else if browser.repository}
              <button type="button" class="creation-row creation-row--primary" onclick={() => void chooseRepository(browser!.path)}><Code2 size={12} />Use this repository</button>
            {/if}
            <div class="max-h-48 overflow-y-auto">
              {#each browser.entries as entry (entry.path)}
                <button type="button" class="creation-row" onclick={() => browserPurpose === "repository" && entry.repository ? void chooseRepository(entry.path) : void browse(entry.path, browserPurpose)}>
                  {#if entry.repository}<Code2 size={12} class="text-content-link" />{:else}<Folder size={12} />{/if}<span class="truncate">{entry.name}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {/if}

    {#if error}
      <p class="creation-error">{error}</p>
    {/if}
  </div>
</form>

<style>
  .code-project-creation { display:flex; min-height:0; flex:1; flex-direction:column; color:rgb(var(--theme-text)); }
  .code-project-creation--popover { width:min(23rem, calc(100vw - 1rem)); max-height:min(34rem, calc(100vh - 2rem)); }
  .creation-header { display:flex; align-items:center; justify-content:space-between; gap:.75rem; border-bottom:1px solid rgb(var(--theme-border)/.22); padding:.65rem .75rem; }
  .creation-body { min-height:0; flex:1; overflow-y:auto; padding:.65rem; }
  .creation-card,.creation-prompt,.creation-source-card,.creation-list,.creation-subpanel,.creation-browser,.creation-branch-panel { border:1px solid rgb(var(--theme-border)/.28); border-radius:var(--theme-container-radius); background:rgb(var(--theme-card)/.62); }
  .creation-card { padding:.65rem; }
  .creation-repo-icon { display:grid; width:1.8rem; height:1.8rem; place-items:center; border-radius:var(--theme-control-radius); background:rgb(var(--theme-pane-muted)/.72); }
  .creation-status { display:flex; align-items:center; gap:.3rem; margin-top:.35rem; color:rgb(var(--theme-warning)); font-size:.65rem; }
  .creation-link { color:rgb(var(--theme-text-tertiary)); font-size:.7rem; }
  .creation-link:hover { color:rgb(var(--theme-text)); }
  .creation-row,.creation-source,.creation-recent { display:flex; width:100%; min-width:0; align-items:center; gap:.5rem; padding:.48rem .55rem; color:rgb(var(--theme-text-secondary)); font-size:.75rem; text-align:left; }
  .creation-row:hover,.creation-source:hover,.creation-recent:hover { background:rgb(var(--theme-card-hover)/.68); color:rgb(var(--theme-text)); }
  .creation-row:disabled,.creation-source:disabled { opacity:.4; }
  .creation-recent+.creation-recent,.creation-row+.creation-row { border-top:1px solid rgb(var(--theme-border)/.18); }
  .creation-separate { display:flex; align-items:center; gap:.4rem; margin-top:.45rem; padding:.4rem .45rem; color:rgb(var(--theme-link)); font-size:.72rem; }
  .creation-prompt { padding:.6rem; }
  .creation-toolbar { display:flex; min-width:0; align-items:center; gap:.5rem; border-top:1px solid rgb(var(--theme-border)/.2); padding-top:.5rem; }
  .creation-branch-trigger { display:flex; min-width:0; flex:1; align-items:center; gap:.35rem; color:rgb(var(--theme-text-tertiary)); font-size:.68rem; }
  .creation-submit { display:grid; width:1.8rem; height:1.8rem; place-items:center; border-radius:999px; background:rgb(var(--theme-action)); color:rgb(var(--on-primary)); }
  .creation-submit:disabled { opacity:.35; }
  .creation-branch-panel,.creation-subpanel { display:grid; gap:.3rem; margin-top:.5rem; padding:.4rem; }
  .creation-back { display:flex; align-items:center; gap:.35rem; padding:.3rem; color:rgb(var(--theme-text-tertiary)); font-size:.7rem; }
  .creation-source-card { display:flex; width:100%; align-items:center; gap:.65rem; padding:.65rem; color:rgb(var(--theme-text)); }
  .creation-source-card:hover { border-color:rgb(var(--theme-link)/.42); background:rgb(var(--theme-card-hover)/.6); }
  .creation-field { width:100%; border:1px solid rgb(var(--theme-border)/.3); border-radius:var(--theme-control-radius); background:rgb(var(--theme-pane)/.72); padding:.4rem .5rem; color:rgb(var(--theme-text)); font-size:.72rem; }
  .creation-use,.creation-provider { border-radius:var(--theme-control-radius); background:rgb(var(--theme-card-hover)/.72); padding:.35rem .55rem; color:rgb(var(--theme-text-secondary)); font-size:.68rem; }
  .creation-provider--active { color:rgb(var(--theme-link)); box-shadow:inset 0 0 0 1px rgb(var(--theme-link)/.35); }
  .creation-use:disabled,.creation-provider:disabled { opacity:.4; }
  .creation-pin,.creation-icon { display:grid; width:1.7rem; height:1.7rem; flex:0 0 auto; place-items:center; border-radius:var(--theme-control-radius); color:rgb(var(--theme-text-tertiary)); }
  .creation-pin:hover,.creation-icon:hover { background:rgb(var(--theme-card-hover)/.7); color:rgb(var(--theme-text)); }
  .creation-browser { margin-top:.5rem; overflow:hidden; }
  .creation-browser-head { display:flex; align-items:center; gap:.35rem; border-bottom:1px solid rgb(var(--theme-border)/.2); padding:.25rem; }
  .creation-row--primary { color:rgb(var(--theme-link)); }
  .creation-error { margin-top:.6rem; border:1px solid rgb(var(--theme-warning)/.3); border-radius:var(--theme-control-radius); background:rgb(var(--theme-warning)/.08); padding:.5rem; color:rgb(var(--theme-warning)); font-size:.7rem; }
</style>
