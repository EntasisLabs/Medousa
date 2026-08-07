<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    ChevronLeft,
    Code2,
    Download,
    FilePlus2,
    Folder,
    FolderOpen,
    FolderPlus,
    GitPullRequestArrow,
    Pin,
    Plus,
    RefreshCw,
    Search,
    X,
  } from "@lucide/svelte";
  import {
    browseForgeRepositories,
    cloneProviderRepository,
    getProviderRepositoryCapabilities,
    gitTargetRepoPath,
    humanPhaseLabel,
    humanizeForgeMessage,
    inspectForgeRepository,
    listForgeRepositories,
    setForgeRepositoryPinned,
    type ItemProjection,
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
  import {
    placeDockPopover,
    type DockPopoverPlacement,
  } from "$lib/utils/dockPopoverPlace";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import CodeRepositoryTree from "$lib/components/lme/explorers/CodeRepositoryTree.svelte";
  import CodeRailSwitcher from "$lib/components/lme/explorers/CodeRailSwitcher.svelte";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";

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
  let searchExpanded = $state(false);
  let fileQuery = $state("");
  let treeLoading = $state(false);
  let treeRef = $state<{
    refreshFiles: () => void;
    startNewFile: () => void;
    startNewFolder: () => void;
    searchInFiles: () => void;
    clearFind: () => void;
  } | null>(null);
  let createOpen = $state(false);
  let createBtnEl = $state<HTMLButtonElement | null>(null);
  let createMenuEl = $state<HTMLDivElement | null>(null);
  let createPlacement = $state<DockPopoverPlacement | null>(null);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  const coLocated = $derived(isCoLocatedWorkshop());
  const currentFolder = $derived(
    coLocated && vault.activeVaultRoot?.path
      ? { path: vault.activeVaultRoot.path, label: vault.activeVaultRoot.label }
      : null,
  );

  function normalizePath(path: string): string {
    return path.replace(/\\/g, "/").replace(/\/+$/, "");
  }

  function repoPathFromWorktree(worktree: string): string | null {
    const normalized = normalizePath(worktree);
    const marker = "/.medousa/worktrees/";
    const idx = normalized.indexOf(marker);
    if (idx > 0) return normalized.slice(0, idx);
    return null;
  }

  function itemRepoPath(item: ItemProjection): string | null {
    const fromTarget = gitTargetRepoPath(item.target);
    if (fromTarget) return fromTarget;
    if (undertakings.detail?.id === item.id) {
      const fromDetail = gitTargetRepoPath(undertakings.detail.target);
      if (fromDetail) return fromDetail;
    }
    const worktree = item.environment?.worktree?.trim()
      ?? (undertakings.detail?.id === item.id
        ? undertakings.detail.environment?.worktree?.trim()
        : undefined);
    if (worktree) {
      const carved = repoPathFromWorktree(worktree);
      if (carved) return carved;
      const needle = normalizePath(worktree);
      for (const entry of repositoryCatalog) {
        const root = normalizePath(entry.path);
        if (needle === root || needle.startsWith(`${root}/`)) return entry.path;
      }
    }
    return null;
  }

  function isActiveThread(item: ItemProjection): boolean {
    return (
      item.human_phase !== "complete" &&
      item.state !== "discarded" &&
      item.state !== "accepted"
    );
  }

  function sortThreads(a: ItemProjection, b: ItemProjection): number {
    const at = a.updated_at ?? a.created_at ?? "";
    const bt = b.updated_at ?? b.created_at ?? "";
    return bt.localeCompare(at);
  }

  function labelForRepo(path: string): string {
    const catalog = repositoryCatalog.find((entry) => entry.path === path);
    return catalog?.display_name ?? rootLabelFromPath(path);
  }

  const activeItems = $derived(
    undertakings.items.filter(isActiveThread).slice().sort(sortThreads),
  );
  const completedItems = $derived(
    undertakings.items.filter((item) => !isActiveThread(item)).slice().sort(sortThreads),
  );
  const selectedItem = $derived(
    undertakings.selectedId
      ? undertakings.items.find((item) => item.id === undertakings.selectedId) ??
          (undertakings.detail?.id === undertakings.selectedId
            ? undertakings.detail
            : null)
      : null,
  );
  const selectedPrepared = $derived(
    Boolean(
      selectedItem &&
        (Boolean(selectedItem.environment) ||
          (undertakings.detail?.id === selectedItem.id &&
            Boolean(undertakings.detail.environment))),
    ),
  );
  const selectedRepoPath = $derived.by(() => {
    if (!selectedItem) return null;
    return (
      itemRepoPath(selectedItem) ||
      (undertakings.detail?.id === selectedItem.id
        ? itemRepoPath(undertakings.detail)
        : null)
    );
  });
  const selectedProjectLabel = $derived.by(() => {
    if (selectedRepoPath) return labelForRepo(selectedRepoPath);
    const detail = undertakings.detail;
    const worktree =
      selectedItem?.environment?.worktree?.trim() ||
      (detail && detail.id === selectedItem?.id
        ? detail.environment?.worktree?.trim()
        : undefined);
    if (worktree) {
      const carved = repoPathFromWorktree(worktree);
      if (carved) return labelForRepo(carved);
    }
    return "Project";
  });
  const selectedThreadLabel = $derived(selectedItem?.title ?? "Thread");
  const fileSearching = $derived(fileQuery.trim().length > 0);
  const showTreeChrome = $derived(Boolean(selectedItem && selectedPrepared));

  const projectSwitcherItems = $derived.by(() => {
    const byPath = new Map<
      string,
      { id: string; label: string; threadCount: number; active: boolean }
    >();
    for (const item of undertakings.items) {
      const path = itemRepoPath(item);
      if (!path) continue;
      const existing = byPath.get(path);
      if (existing) existing.threadCount += 1;
      else {
        byPath.set(path, {
          id: path,
          label: labelForRepo(path),
          threadCount: 1,
          active: path === selectedRepoPath,
        });
      }
    }
    for (const entry of repositoryCatalog.filter((e) => e.available)) {
      if (byPath.has(entry.path)) continue;
      byPath.set(entry.path, {
        id: entry.path,
        label: entry.display_name,
        threadCount: 0,
        active: entry.path === selectedRepoPath,
      });
    }
    return [...byPath.values()]
      .sort((a, b) => a.label.localeCompare(b.label))
      .map((entry) => ({
        id: entry.id,
        label: entry.label,
        detail:
          entry.threadCount === 0
            ? "No threads yet"
            : `${entry.threadCount} ${entry.threadCount === 1 ? "thread" : "threads"}`,
        active: entry.active,
      }));
  });

  const threadSwitcherItems = $derived.by(() => {
    const preferred = selectedRepoPath
      ? activeItems.filter((item) => itemRepoPath(item) === selectedRepoPath)
      : [];
    const otherActive = activeItems.filter(
      (item) => itemRepoPath(item) !== selectedRepoPath,
    );
    const finished = completedItems;
    const ordered = [...preferred, ...otherActive, ...finished];
    return ordered.map((item) => {
      const repo = itemRepoPath(item);
      const sameRepo = Boolean(selectedRepoPath && repo === selectedRepoPath);
      return {
        id: item.id,
        label: item.title,
        detail: sameRepo
          ? humanPhaseLabel(item.human_phase)
          : `${repo ? labelForRepo(repo) : "Project"} · ${humanPhaseLabel(item.human_phase)}`,
        active: item.id === selectedItem?.id,
      };
    });
  });

  const recentRepositories = $derived(repositoryCatalog.filter((entry) => entry.available));
  const duplicateNeedsChoice = $derived(
    Boolean(repository?.existing_projects.length && !duplicateAcknowledged),
  );
  const repositoryReady = $derived(
    Boolean(repository && repository.has_commits !== false && baseRef.trim()),
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

  async function selectProject(path: string) {
    const threads = undertakings.items
      .filter((item) => itemRepoPath(item) === path)
      .slice()
      .sort(sortThreads);
    const preferred = threads.find(isActiveThread) ?? threads[0];
    if (preferred) {
      await openItem(preferred.id, preferred.title);
      return;
    }
    await chooseRepository(path);
  }

  async function openReview() {
    if (!selectedItem || selectedItem.human_phase !== "review") return;
    await lmeWorkspace.openCodeReview(selectedItem.id, selectedItem.title);
  }

  function placeCreateMenu() {
    if (!createBtnEl) return;
    createPlacement = placeDockPopover(createBtnEl, {
      preferUp: false,
      width: 196,
      maxHeight: 280,
    });
  }

  function closeMenus() {
    createOpen = false;
    createPlacement = null;
  }

  function toggleCreateMenu(event: MouseEvent) {
    event.stopPropagation();
    if (createOpen) {
      closeMenus();
      return;
    }
    createOpen = true;
    requestAnimationFrame(placeCreateMenu);
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  function onCreatePointerDown(event: PointerEvent) {
    if (!createOpen) return;
    const target = event.target as Node;
    if (createBtnEl?.contains(target) || createMenuEl?.contains(target)) return;
    closeMenus();
  }

  $effect(() => {
    if (!createOpen) return;
    window.addEventListener("pointerdown", onCreatePointerDown);
    window.addEventListener("resize", placeCreateMenu);
    return () => {
      window.removeEventListener("pointerdown", onCreatePointerDown);
      window.removeEventListener("resize", placeCreateMenu);
    };
  });

  $effect(() => {
    if (fileSearching && !searchExpanded) searchExpanded = true;
  });

  $effect(() => {
    if (showTreeChrome) return;
    if (!searchExpanded && !fileQuery) return;
    searchExpanded = false;
    fileQuery = "";
  });

  async function openSearch() {
    closeMenus();
    await ensureRailPopoverOpen();
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
    searchInputEl?.select();
  }

  function closeSearch() {
    searchExpanded = false;
    if (fileSearching) {
      fileQuery = "";
      treeRef?.clearFind();
    }
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      treeRef?.searchInFiles();
    }
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
      baseRef = repository.suggested_base_ref ?? "";
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
      baseRef = cloned.suggested_base_ref ?? "";
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

<aside class="flex h-full min-h-0 w-full flex-col" aria-label="Code">
  <header class="lme-side-rail-dock lme-code-dock" use:portLmeDock>
    {#if searchExpanded && showTreeChrome}
      <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
        <Search size={14} strokeWidth={1.75} class="shrink-0 text-content-quiet" aria-hidden="true" />
        <input
          bind:this={searchInputEl}
          class="lme-code-dock-search min-w-0 flex-1 border-0 bg-transparent placeholder:text-content-quiet focus:outline-none focus:ring-0"
          type="search"
          placeholder="Find a file…"
          bind:value={fileQuery}
          onkeydown={handleSearchKeydown}
        />
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Close search"
          title="Close search"
          onclick={closeSearch}
        >
          <X size={14} strokeWidth={1.75} />
        </button>
      </div>
    {:else}
      <div
        class="lme-dock-chrome-secondary lme-dock-chrome-secondary--crumb flex min-w-0 items-center gap-0.5"
      >
        <CodeRailSwitcher
          label="Project"
          value={selectedProjectLabel}
          title="Switch project"
          soft={!selectedRepoPath && selectedProjectLabel === "Project"}
          items={projectSwitcherItems}
          emptyHint="No projects yet"
          onSelect={(id) => void selectProject(id)}
        />
        <span
          class="nav-rail-dock-crumb-sep lme-code-dock-sep shrink-0 px-px leading-none"
          aria-hidden="true"
        >/</span>
        <CodeRailSwitcher
          label="Thread"
          value={selectedThreadLabel}
          title="Switch thread"
          soft={!selectedItem}
          items={threadSwitcherItems}
          emptyHint="No threads yet"
          onSelect={(id) => {
            const item = undertakings.items.find((entry) => entry.id === id);
            if (item) void openItem(item.id, item.title);
          }}
        />
      </div>
      <div class="lme-dock-chrome-secondary lme-dock-chrome-secondary--spacer min-w-1 flex-1"></div>

      {#if selectedItem && !selectedPrepared && selectedItem.allowed_actions.provision.allowed}
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Set up project"
          title="Set up project"
          onclick={() => void openItem(selectedItem.id, selectedItem.title)}
        >
          <FolderOpen size={15} strokeWidth={1.75} />
        </button>
      {/if}

      {#if selectedItem?.human_phase === "review"}
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Open review"
          title="Open review"
          onclick={() => void openReview()}
        >
          <GitPullRequestArrow size={15} strokeWidth={1.75} />
        </button>
      {/if}

      <div class="relative shrink-0">
        <button
          bind:this={createBtnEl}
          type="button"
          class="vault-dock-icon-btn"
          aria-haspopup="menu"
          aria-expanded={createOpen}
          aria-label="New"
          title="New"
          onclick={toggleCreateMenu}
        >
          <Plus size={16} strokeWidth={1.75} />
        </button>
      </div>
      {#if createOpen && createPlacement}
        <BodyPortal>
          <div
            bind:this={createMenuEl}
            class="vault-dock-popover"
            role="menu"
            tabindex="-1"
            style:left="{createPlacement.left}px"
            style:top="{createPlacement.top}px"
            style:width="{createPlacement.width}px"
            style:max-height="{createPlacement.maxHeight}px"
            style:transform={createPlacement.transform}
            onclick={(event) => event.stopPropagation()}
            onkeydown={handleMenuKeydown}
          >
            <button
              type="button"
              role="menuitem"
              class="vault-menu-item"
              onclick={() => {
                closeMenus();
                creating = true;
              }}
            >
              <Plus size={14} strokeWidth={2} />
              New thread
            </button>
            {#if showTreeChrome}
              <div class="vault-dock-popover__sep"></div>
              <button
                type="button"
                role="menuitem"
                class="vault-menu-item"
                onclick={() => {
                  closeMenus();
                  treeRef?.startNewFile();
                }}
              >
                <FilePlus2 size={14} strokeWidth={2} />
                New file
              </button>
              <button
                type="button"
                role="menuitem"
                class="vault-menu-item"
                onclick={() => {
                  closeMenus();
                  treeRef?.startNewFolder();
                }}
              >
                <FolderPlus size={14} strokeWidth={2} />
                New folder
              </button>
            {/if}
          </div>
        </BodyPortal>
      {/if}

      {#if showTreeChrome}
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Refresh files"
          title="Refresh files"
          disabled={treeLoading}
          onclick={() => treeRef?.refreshFiles()}
        >
          <RefreshCw size={14} strokeWidth={1.75} class={treeLoading ? "animate-spin" : ""} />
        </button>
        <button
          type="button"
          class="vault-dock-icon-btn {fileSearching ? 'vault-dock-icon-btn-active' : ''}"
          aria-label="Find a file"
          title="Find"
          onclick={() => void openSearch()}
        >
          <Search size={15} strokeWidth={1.75} />
        </button>
      {/if}
    {/if}
  </header>

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
              <p class="truncate font-mono text-[9px] text-content-quiet">{repository.path}</p>
            </div>
            <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[9px] text-content-tertiary hover:bg-surface-800" onclick={() => {
              repository = null;
              duplicateAcknowledged = false;
            }}>Change</button>
          </div>
          <p class="mt-1 text-[9px] leading-relaxed {repository.dirty ? 'text-amber-200' : 'text-content-quiet'}">
            {repository.state_explanation}
          </p>
          <details class="mt-1 text-[9px] text-content-faint">
            <summary class="cursor-pointer select-none hover:text-content-tertiary">What Medousa can do here</summary>
            <p class="mt-1 leading-relaxed">{repository.trust_explanation}</p>
          </details>
        </div>
        {#if repository.existing_projects.length > 0 && !duplicateAcknowledged}
          <div class="rounded border border-primary-500/30 bg-primary-950/15 p-2">
            <p class="text-[10px] font-medium text-surface-200">You already have work here</p>
            <p class="mt-0.5 text-[9px] leading-relaxed text-content-quiet">Continue it, or deliberately start a separate change.</p>
            <div class="mt-1.5 flex flex-col gap-1">
              {#each repository.existing_projects.slice(0, 3) as existing (existing.id)}
                <button type="button" class="flex items-center justify-between gap-2 rounded px-2 py-1 text-left text-[10px] text-content-secondary hover:bg-surface-800" onclick={() => void openItem(existing.id, existing.title)}>
                  <span class="min-w-0 flex-1 truncate">{existing.title}</span>
                  <span class="shrink-0 text-[8px] text-content-quiet">{humanPhaseLabel(existing.human_phase)}</span>
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
              <FolderOpen size={13} class="text-content-link" />Choose a folder…
            </button>
            {#if currentFolder}
              <button type="button" class="flex min-w-0 items-center gap-2 rounded px-2 py-1 text-left text-[10px] text-content-tertiary hover:bg-surface-800 hover:text-surface-100" onclick={() => void chooseRepository(currentFolder.path)}>
                <FolderOpen size={11} class="shrink-0" /><span class="min-w-0 flex-1 truncate">Current folder · {currentFolder.label}</span>
              </button>
            {/if}
          {:else}
            <button type="button" class="flex items-center gap-2 rounded border border-surface-500/30 px-2 py-2 text-left text-[10px] text-surface-200 hover:bg-surface-800" onclick={() => void browseRepositoryFolder(null, "repository")}>
              <FolderOpen size={13} class="text-content-link" />Browse connected computer…
            </button>
          {/if}
          <button type="button" class="flex items-center gap-2 rounded px-2 py-1 text-left text-[10px] text-content-tertiary hover:bg-surface-800 hover:text-surface-100" onclick={() => void openHostedRepository()}>
            <Download size={11} class="shrink-0" />
            <span>{hostedOpen ? "Hide hosted repositories" : "Clone from GitHub or GitLab…"}</span>
          </button>
          {#if hostedOpen}
            <div class="grid gap-1.5 rounded border border-surface-500/25 bg-surface-900/35 p-2">
              <div class="flex gap-1">
                {#each hostedAdapters as adapter (adapter.provider)}
                  <button
                    type="button"
                    class="rounded px-2 py-1 text-[9px] {hostedProvider === adapter.provider ? 'bg-surface-700 text-surface-100' : 'text-content-quiet hover:bg-surface-800'}"
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
                <span class="min-w-0 flex-1 truncate font-mono text-[8px] text-content-faint">
                  {hostedParent || "Choose where to keep it"}
                </span>
                {#if coLocated}
                  <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[9px] text-content-tertiary hover:bg-surface-800" onclick={() => void pickHostedParent()}>Choose…</button>
                {:else}
                  <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[9px] text-content-tertiary hover:bg-surface-800" onclick={() => void browseRepositoryFolder(hostedParent || null, "destination")}>Choose…</button>
                {/if}
              </div>
              {#if hostedAdapters.length && !hostedAdapters.some((adapter) => adapter.available)}
                <p class="text-[9px] leading-relaxed text-content-quiet">
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
              <button type="button" class="flex min-w-0 flex-1 items-center gap-2 px-2 py-1 text-left text-[10px] text-content-tertiary hover:text-surface-100" onclick={() => void chooseRepository(recent.path)}>
                <Code2 size={11} class="shrink-0" />
                <span class="min-w-0 flex-1 truncate">{recent.display_name}</span>
                <span class="truncate text-[8px] text-content-faint">{recent.has_commits === false ? "No commits" : recent.current_branch ?? recent.suggested_base_ref ?? "Repository"}</span>
              </button>
              <button
                type="button"
                class="mr-1 shrink-0 rounded p-0.5 {recent.pinned ? 'text-content-link' : 'text-content-faint opacity-0 group-hover:opacity-100'}"
                aria-label={recent.pinned ? `Unpin ${recent.display_name}` : `Pin ${recent.display_name}`}
                onclick={(event) => void togglePinned(recent, event)}
              ><Pin size={10} fill={recent.pinned ? "currentColor" : "none"} /></button>
            </div>
          {/each}
          {#if !coLocated}
            <details class="px-1 text-[9px] text-content-faint">
              <summary class="cursor-pointer select-none hover:text-content-tertiary">Enter a path instead</summary>
              <div class="mt-1 flex gap-1">
                <input class="code-field min-w-0 flex-1" placeholder="Folder on connected computer" bind:value={repoPath} />
                <button type="button" class="rounded border border-surface-500/35 px-2 text-[10px] text-content-secondary" disabled={!repoPath.trim() || inspecting} onclick={() => void chooseRepository(repoPath)}>Use</button>
              </div>
            </details>
          {/if}
        </div>
      {/if}
      {#if browserOpen && !repository}
        <div class="overflow-hidden rounded border border-surface-500/35 bg-surface-950/80">
          <div class="flex items-center gap-1 border-b border-surface-500/25 px-1.5 py-1">
            <button type="button" class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-200 disabled:opacity-30" aria-label="Parent folder" disabled={!repositoryBrowser?.parent || browserLoading} onclick={() => void browseRepositoryFolder(repositoryBrowser?.parent)}><ChevronLeft size={12} /></button>
            <p class="min-w-0 flex-1 truncate font-mono text-[9px] text-content-tertiary">{repositoryBrowser?.path ?? "Connected computer"}</p>
            <button type="button" class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-200" aria-label="Close repository browser" onclick={() => (browserOpen = false)}>×</button>
          </div>
          {#if browserLoading}
            <p class="px-2 py-3 text-[10px] text-content-quiet">Looking for projects…</p>
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
                  <button type="button" class="shrink-0 rounded px-1.5 py-0.5 text-[8px] text-content-quiet hover:bg-surface-800 hover:text-surface-200" title={place.path} onclick={() => void browseRepositoryFolder(place.path)}>{place.name}</button>
                {/each}
              </div>
            {/if}
            <div class="max-h-48 overflow-y-auto py-1">
              {#if repositoryBrowser.entries.length === 0}
                <p class="px-2 py-3 text-[10px] text-content-quiet">No folders here.</p>
              {:else}
                {#each repositoryBrowser.entries as entry (entry.path)}
                  <button type="button" class="flex w-full items-center gap-2 px-2 py-1 text-left text-[10px] hover:bg-surface-800" onclick={() => browserPurpose === "repository" && entry.repository ? void chooseRepository(entry.path) : void browseRepositoryFolder(entry.path, browserPurpose)}>
                    {#if entry.repository}<Code2 size={11} class="shrink-0 text-content-link" />{:else}<Folder size={11} class="shrink-0 text-content-quiet" />{/if}
                    <span class="min-w-0 flex-1 truncate {entry.repository ? 'text-surface-200' : 'text-content-tertiary'}">{entry.name}</span>
                    {#if entry.repository}<span class="text-[8px] text-content-link/70">Repository</span>{/if}
                  </button>
                {/each}
              {/if}
            </div>
            {#if repositoryBrowser.truncated}
              <p class="border-t border-surface-500/20 px-2 py-1 text-[8px] text-content-faint">Showing the first 500 folders.</p>
            {/if}
          {/if}
        </div>
      {/if}
      <label class="code-field-label">
        <span>What do you want to accomplish?</span>
        <textarea class="code-field min-h-16 resize-none" placeholder="Make indexing cancellation-safe" bind:value={outcome}></textarea>
      </label>
      {#if repository}
        <details class="text-[9px] text-content-quiet">
          <summary class="cursor-pointer select-none hover:text-content-secondary">Starting point</summary>
          <label class="mt-1 flex items-center gap-2">
            <span class="shrink-0">Branch</span>
            <input class="code-field min-w-0 flex-1" bind:value={baseRef} />
          </label>
        </details>
      {/if}
      <button
        type="submit"
        class="rounded bg-primary-500/80 px-2 py-1 text-xs font-medium text-surface-50 disabled:opacity-40"
        disabled={busy || inspecting || !outcome.trim() || !repositoryReady || duplicateNeedsChoice}
      >{busy ? "Preparing project…" : inspecting ? "Reading project…" : "Start"}</button>
    </form>
  {/if}

  {#if error || undertakings.error}
    <p class="m-2 rounded border border-amber-500/35 bg-amber-950/25 px-2 py-1 text-[10px] text-amber-100">
      {humanizeForgeMessage(error || undertakings.error || "")}
    </p>
  {/if}

  <div class="flex min-h-0 flex-1 flex-col">
    {#if undertakings.loading && undertakings.items.length === 0}
      <p class="px-3 py-3 text-xs text-content-quiet">Loading threads…</p>
    {:else if undertakings.items.length === 0}
      <div class="px-3 py-5 text-center">
        <Code2 size={20} class="mx-auto text-content-faint" />
        <p class="mt-2 text-sm font-medium text-surface-200">No threads yet</p>
        <p class="workshop-faint mt-1 text-[10px] leading-relaxed">
          Start with a repository and the change you want to make. Medousa will keep the work together.
        </p>
      </div>
    {:else if selectedItem}
      <div class="min-h-0 flex-1 overflow-hidden">
        <CodeRepositoryTree
          bind:this={treeRef}
          workId={selectedItem.id}
          prepared={selectedPrepared}
          chromeInDock
          bind:query={fileQuery}
          bind:loading={treeLoading}
          fill
        />
      </div>
    {:else}
      <div class="min-h-0 flex-1 overflow-y-auto py-1.5">
        {#if activeItems.length}
          <p class="px-3 pb-1 pt-1 text-[9px] font-medium uppercase tracking-wider text-content-quiet">In progress</p>
          {#each activeItems as item (item.id)}
            <button
              type="button"
              class="w-full px-3 py-1.5 text-left transition hover:bg-surface-800/70"
              onclick={() => void openItem(item.id, item.title)}
            >
              <span class="block truncate text-xs font-medium text-surface-100">{item.title}</span>
              <span class="mt-0.5 block truncate text-[9px] text-content-secondary">
                {#if itemRepoPath(item)}{labelForRepo(itemRepoPath(item)!)} · {/if}{humanPhaseLabel(item.human_phase)}
              </span>
            </button>
          {/each}
        {/if}
        {#if completedItems.length}
          <p class="px-3 pb-1 pt-3 text-[9px] font-medium uppercase tracking-wider text-content-quiet">Finished</p>
          {#each completedItems as item (item.id)}
            <button
              type="button"
              class="w-full px-3 py-1.5 text-left transition hover:bg-surface-800/70"
              onclick={() => void openItem(item.id, item.title)}
            >
              <span class="block truncate text-xs font-medium text-surface-200">{item.title}</span>
              <span class="mt-0.5 block truncate text-[9px] text-content-secondary">
                {#if itemRepoPath(item)}{labelForRepo(itemRepoPath(item)!)} · {/if}{humanPhaseLabel(item.human_phase)}
              </span>
            </button>
          {/each}
        {/if}
      </div>
    {/if}
  </div>
</aside>

<style>
  /* Match Code tree workbench type in the dock strip. */
  :global(.lme-code-dock) {
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(.lme-code-dock .vault-dock-branch) {
    max-width: 10rem;
    height: 1.75rem;
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 92%,
      rgb(var(--theme-text-secondary))
    );
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 500;
    letter-spacing: 0;
    line-height: 1.2;
  }

  :global(.lme-code-dock .vault-dock-branch:hover),
  :global(.lme-code-dock .vault-dock-branch--active),
  :global(.lme-code-dock .vault-dock-branch[aria-expanded="true"]) {
    color: rgb(var(--theme-text));
  }

  :global(.lme-code-dock .vault-dock-branch__label) {
    color: inherit;
  }

  :global(.lme-code-dock .lme-code-dock-sep) {
    color: color-mix(in srgb, rgb(var(--theme-text)) 40%, transparent);
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 500;
  }

  :global(.lme-code-dock .lme-code-dock-search) {
    color: rgb(var(--theme-text));
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 400;
    letter-spacing: 0;
    line-height: 1.2;
  }

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
    color: rgb(var(--theme-text-quiet));
    font-size: 0.58rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
</style>
