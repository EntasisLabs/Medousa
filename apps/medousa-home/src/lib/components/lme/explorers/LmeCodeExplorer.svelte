<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    Archive,
    ArchiveRestore,
    ChevronDown,
    ChevronRight,
    CircleDot,
    Code2,
    FilePlus2,
    Folder,
    FolderOpen,
    FolderPlus,
    GitPullRequestArrow,
    HardDriveDownload,
    MoreHorizontal,
    Pin,
    Plus,
    RefreshCw,
    Search,
    X,
  } from "@lucide/svelte";
  import {
    gitTargetRepoPath,
    humanPhaseLabel,
    humanizeForgeMessage,
    listForgeRepositories,
    setForgeRepositoryArchived,
    setForgeRepositoryPinned,
    type ItemProjection,
    type RepositoryCatalogEntry,
  } from "$lib/forge";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { closeUndertaking } from "$lib/utils/undertakingWorkspace";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { rootLabelFromPath } from "$lib/utils/externalDeskApi";
  import { revealFileInFinder } from "$lib/utils/vaultFilesystem";
  import {
    placeDockPopover,
    type DockPopoverPlacement,
  } from "$lib/utils/dockPopoverPlace";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import CodeRepositoryTree from "$lib/components/lme/explorers/CodeRepositoryTree.svelte";
  import CodeProjectCreationFlow from "$lib/components/code/CodeProjectCreationFlow.svelte";
  import CodeRailSwitcher from "$lib/components/lme/explorers/CodeRailSwitcher.svelte";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";

  let creating = $state(false);
  let creationRepoPath = $state<string | null>(null);
  let busy = $state(false);
  let repositoryCatalog = $state<RepositoryCatalogEntry[]>([]);
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
  let expandedProjects = $state<Record<string, boolean>>({});
  let expandedThreadLists = $state<Record<string, boolean>>({});
  let archivedExpanded = $state(false);
  let projectMenuOpen = $state<string | null>(null);
  let threadMenuOpen = $state<string | null>(null);

  const THREAD_PREVIEW_LIMIT = 5;

  type ProjectGroup = {
    id: string;
    path: string | null;
    label: string;
    available: boolean;
    archived: boolean;
    pinned: boolean;
    threads: ItemProjection[];
    activeThreads: ItemProjection[];
    completedThreads: ItemProjection[];
    workspaceThreads: ItemProjection[];
  };

  const coLocated = $derived(isCoLocatedWorkshop());

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
  const projectGroups = $derived.by(() => {
    const byId = new Map<string, ProjectGroup>();
    for (const item of undertakings.items) {
      const path = itemRepoPath(item);
      const id = path ?? `work:${item.id}`;
      let group = byId.get(id);
      if (!group) {
        const catalog = path
          ? repositoryCatalog.find((entry) => entry.path === path)
          : undefined;
        group = {
          id,
          path,
          label: path ? labelForRepo(path) : "Project",
          available: catalog?.available ?? true,
          archived: catalog?.archived ?? false,
          pinned: catalog?.pinned ?? false,
          threads: [],
          activeThreads: [],
          completedThreads: [],
          workspaceThreads: [],
        };
        byId.set(id, group);
      }
      group.threads.push(item);
      if (isActiveThread(item)) {
        group.activeThreads.push(item);
        if (
          item.workspace_present ?? Boolean(item.environment?.worktree?.trim())
        ) {
          group.workspaceThreads.push(item);
        }
      } else {
        group.completedThreads.push(item);
      }
    }
    return [...byId.values()]
      .map((group) => ({
        ...group,
        threads: group.threads.slice().sort(sortThreads),
        activeThreads: group.activeThreads.slice().sort(sortThreads),
        completedThreads: group.completedThreads.slice().sort(sortThreads),
        workspaceThreads: group.workspaceThreads.slice().sort(sortThreads),
      }))
      .sort((a, b) => {
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
        const aTime = a.threads[0]?.updated_at ?? a.threads[0]?.created_at ?? "";
        const bTime = b.threads[0]?.updated_at ?? b.threads[0]?.created_at ?? "";
        return bTime.localeCompare(aTime) || a.label.localeCompare(b.label);
      });
  });
  const activeProjectGroups = $derived(projectGroups.filter((group) => !group.archived));
  const archivedProjectGroups = $derived(projectGroups.filter((group) => group.archived));
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
      const closable =
        item.allowed_actions?.discard?.allowed !== false && isActiveThread(item);
      return {
        id: item.id,
        label: item.title,
        detail: sameRepo
          ? humanPhaseLabel(item.human_phase)
          : `${repo ? labelForRepo(repo) : "Project"} · ${humanPhaseLabel(item.human_phase)}`,
        active: item.id === selectedItem?.id,
        closable,
      };
    });
  });

  onMount(() => {
    void undertakings.refreshList();
    void loadRepositoryCatalog();
  });

  $effect(() => {
    if (!lmeWorkspace.codeCreateRequested) return;
    creationRepoPath = null;
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

  function projectExpanded(id: string): boolean {
    return expandedProjects[id] !== false;
  }

  function toggleProject(id: string) {
    expandedProjects = { ...expandedProjects, [id]: !projectExpanded(id) };
  }

  function visibleProjectThreads(group: ProjectGroup): ItemProjection[] {
    const ordered = [...group.activeThreads, ...group.completedThreads];
    return expandedThreadLists[group.id]
      ? ordered
      : ordered.slice(0, THREAD_PREVIEW_LIMIT);
  }

  function projectStorageLabel(group: ProjectGroup): string {
    if (!group.available) return "Source missing";
    if (group.workspaceThreads.length > 0) {
      return `${group.workspaceThreads.length} ${group.workspaceThreads.length === 1 ? "workspace" : "workspaces"}`;
    }
    if (group.activeThreads.length > 0) return "Not set up";
    return "Workspaces released";
  }

  async function setProjectArchived(group: ProjectGroup, archived: boolean) {
    if (!group.path || busy) return;
    busy = true;
    error = null;
    projectMenuOpen = null;
    try {
      repositoryCatalog = await setForgeRepositoryArchived(group.path, archived);
      if (archived) archivedExpanded = true;
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function setProjectPinned(group: ProjectGroup, pinned: boolean) {
    if (!group.path || busy) return;
    busy = true;
    error = null;
    projectMenuOpen = null;
    try {
      repositoryCatalog = await setForgeRepositoryPinned(group.path, pinned);
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function revealProject(path: string) {
    projectMenuOpen = null;
    try {
      await revealFileInFinder(path);
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    }
  }

  async function closeThread(id: string) {
    const item =
      undertakings.items.find((entry) => entry.id === id) ??
      (undertakings.detail?.id === id ? undertakings.detail : null);
    if (!item) return;
    if (
      !window.confirm(
        `Release the workspace for “${item.title}”?\n\nIts managed working copy will be removed and this active change will be discarded. Chat history and the source repository stay available.`,
      )
    ) {
      return;
    }
    busy = true;
    error = null;
    try {
      await closeUndertaking(item);
      await undertakings.refreshList();
      if (undertakings.selectedId === id) {
        const next = undertakings.items
          .filter((entry) => entry.id !== id && isActiveThread(entry))
          .slice()
          .sort(sortThreads)[0];
        if (next) {
          await openItem(next.id, next.title);
        } else {
          undertakings.clearActive();
          await undertakings.select("");
        }
      }
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function releaseProjectWorkspaces(group: ProjectGroup) {
    const releasable = group.workspaceThreads.filter(
      (item) => item.allowed_actions?.discard?.allowed !== false,
    );
    if (releasable.length === 0 || busy) return;
    const noun = releasable.length === 1 ? "workspace" : "workspaces";
    if (
      !window.confirm(
        `Release ${releasable.length} ${noun} for “${group.label}”?\n\nMedousa will remove the managed working ${releasable.length === 1 ? "copy" : "copies"} and discard the associated active ${releasable.length === 1 ? "change" : "changes"}. Chat history and the source repository stay available.`,
      )
    ) {
      return;
    }
    busy = true;
    error = null;
    projectMenuOpen = null;
    try {
      for (const item of releasable) await closeUndertaking(item);
      await undertakings.refreshList();
      if (selectedItem && releasable.some((item) => item.id === selectedItem.id)) {
        undertakings.clearActive();
        await undertakings.select("");
      }
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
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
    creationRepoPath = path;
    creating = true;
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
          onClose={(id) => void closeThread(id)}
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
                creationRepoPath = null;
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
    <CodeProjectCreationFlow
      presentation="rail"
      initialRepositoryPath={creationRepoPath}
      onCancel={() => {
        creating = false;
        creationRepoPath = null;
      }}
      onCreated={async (item) => {
        creating = false;
        await loadRepositoryCatalog();
        await lmeWorkspace.openCodeWorkspace(item.id, item.title);
      }}
      onContinue={async (item) => {
        creating = false;
        await lmeWorkspace.openCodeWorkspace(item.id, item.title);
      }}
      onCatalogChanged={loadRepositoryCatalog}
    />
  {/if}
  {#if error || undertakings.error}
    <p class="m-2 rounded border border-amber-500/35 bg-amber-950/25 px-2 py-1 text-[10px] text-amber-100">
      {humanizeForgeMessage(error || undertakings.error || "")}
    </p>
  {/if}

  {#snippet projectGroup(group: ProjectGroup)}
    <section class="code-project-group">
      <div class="code-project-heading group/project">
        <button
          type="button"
          class="code-project-toggle"
          aria-expanded={projectExpanded(group.id)}
          onclick={() => toggleProject(group.id)}
        >
          {#if projectExpanded(group.id)}
            <ChevronDown size={12} strokeWidth={1.75} class="code-project-chevron" />
            <FolderOpen size={14} strokeWidth={1.75} class="code-project-folder" />
          {:else}
            <ChevronRight size={12} strokeWidth={1.75} class="code-project-chevron" />
            <Folder size={14} strokeWidth={1.75} class="code-project-folder" />
          {/if}
          <span class="code-project-name">{group.label}</span>
          <span class="code-project-storage" class:code-project-storage--missing={!group.available}>
            {projectStorageLabel(group)}
          </span>
        </button>
        <OverflowMenu
          open={projectMenuOpen === group.id}
          onOpenChange={(open) => (projectMenuOpen = open ? group.id : null)}
          panelWidth={196}
          panelClass="code-project-action-menu"
          label={`Actions for ${group.label}`}
        >
          {#snippet trigger({ open, toggle })}
            <button
              type="button"
              class="code-project-more"
              class:code-project-more--open={open}
              aria-label={`Actions for ${group.label}`}
              title="Project actions"
              aria-haspopup="menu"
              aria-expanded={open}
              onclick={(event) => {
                event.stopPropagation();
                toggle();
              }}
            ><MoreHorizontal size={14} strokeWidth={1.75} /></button>
          {/snippet}
          {#if group.path && !group.archived}
            <button
              type="button"
              role="menuitem"
              class="code-project-menu-item"
              disabled={busy}
              onclick={() => void setProjectPinned(group, !group.pinned)}
            >
              <Pin size={13} strokeWidth={1.75} fill={group.pinned ? "currentColor" : "none"} />
              {group.pinned ? "Unpin project" : "Pin project"}
            </button>
          {/if}
          {#if group.path && group.available && coLocated}
            <button
              type="button"
              role="menuitem"
              class="code-project-menu-item"
              onclick={() => void revealProject(group.path!)}
            >
              <FolderOpen size={13} strokeWidth={1.75} />
              Reveal in Finder
            </button>
          {/if}
          {#if group.path && group.available && !group.archived}
            <button
              type="button"
              role="menuitem"
              class="code-project-menu-item"
              disabled={busy}
              onclick={() => {
                projectMenuOpen = null;
                creationRepoPath = group.path!;
                creating = true;
              }}
            >
              <Plus size={13} strokeWidth={1.75} />
              New thread
            </button>
          {/if}
          {#if group.workspaceThreads.length > 0}
            <button
              type="button"
              role="menuitem"
              class="code-project-menu-item"
              disabled={busy}
              onclick={() => void releaseProjectWorkspaces(group)}
            >
              <HardDriveDownload size={13} strokeWidth={1.75} />
              Release {group.workspaceThreads.length === 1 ? "workspace" : `${group.workspaceThreads.length} workspaces`}…
            </button>
          {/if}
          {#if group.path}
            <button
              type="button"
              role="menuitem"
              class="code-project-menu-item"
              disabled={busy}
              onclick={() => void setProjectArchived(group, !group.archived)}
            >
              {#if group.archived}<ArchiveRestore size={13} strokeWidth={1.75} />{:else}<Archive size={13} strokeWidth={1.75} />{/if}
              {group.archived ? "Restore project" : "Archive project"}
            </button>
          {/if}
        </OverflowMenu>
      </div>

      {#if projectExpanded(group.id)}
        <div class="code-project-threads">
          {#each visibleProjectThreads(group) as item (item.id)}
            <div class="code-thread-row group/thread" class:code-thread-row--selected={item.id === selectedItem?.id}>
              <button
                type="button"
                class="code-thread-open"
                onclick={() => void openItem(item.id, item.title)}
              >
                <CircleDot
                  size={10}
                  strokeWidth={1.75}
                  class={isActiveThread(item) ? "is-active" : "is-finished"}
                />
                <span class="code-thread-copy">
                  <span class="code-thread-title">{item.title}</span>
                  <span class="code-thread-status">{humanPhaseLabel(item.human_phase)}</span>
                </span>
              </button>
              {#if isActiveThread(item) && (item.workspace_present ?? Boolean(item.environment?.worktree?.trim())) && item.allowed_actions?.discard?.allowed !== false}
                <OverflowMenu
                  open={threadMenuOpen === item.id}
                  onOpenChange={(open) => (threadMenuOpen = open ? item.id : null)}
                  panelWidth={196}
                  panelClass="code-project-action-menu"
                  label={`Actions for ${item.title}`}
                >
                  {#snippet trigger({ open, toggle })}
                    <button
                      type="button"
                      class="code-thread-more"
                      class:code-thread-more--open={open}
                      aria-label={`Actions for ${item.title}`}
                      title="Thread actions"
                      aria-haspopup="menu"
                      aria-expanded={open}
                      onclick={(event) => {
                        event.stopPropagation();
                        toggle();
                      }}
                    ><MoreHorizontal size={13} strokeWidth={1.75} /></button>
                  {/snippet}
                  <button
                    type="button"
                    role="menuitem"
                    class="code-project-menu-item"
                    disabled={busy}
                    onclick={() => {
                      threadMenuOpen = null;
                      void closeThread(item.id);
                    }}
                  >
                    <HardDriveDownload size={13} strokeWidth={1.75} />
                    Release workspace…
                  </button>
                </OverflowMenu>
              {/if}
            </div>
          {/each}
          {#if group.threads.length > THREAD_PREVIEW_LIMIT}
            <button
              type="button"
              class="code-project-show-more"
              onclick={() => (expandedThreadLists = {
                ...expandedThreadLists,
                [group.id]: !expandedThreadLists[group.id],
              })}
            >
              {expandedThreadLists[group.id]
                ? "Show less"
                : `Show ${group.threads.length - THREAD_PREVIEW_LIMIT} more`}
            </button>
          {/if}
        </div>
      {/if}
    </section>
  {/snippet}

  <div class="{creating ? 'hidden' : 'flex'} min-h-0 flex-1 flex-col">
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
      <div class="code-project-list min-h-0 flex-1 overflow-y-auto py-2">
        <p class="code-project-section-label">Projects</p>
        {#each activeProjectGroups as group (group.id)}
          {@render projectGroup(group)}
        {/each}
        {#if archivedProjectGroups.length}
          <button
            type="button"
            class="code-archived-heading"
            aria-expanded={archivedExpanded}
            onclick={() => (archivedExpanded = !archivedExpanded)}
          >
            {#if archivedExpanded}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
            <span>Archived</span>
            <span class="code-archived-count">{archivedProjectGroups.length}</span>
          </button>
          {#if archivedExpanded}
            {#each archivedProjectGroups as group (group.id)}
              {@render projectGroup(group)}
            {/each}
          {/if}
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

  .code-project-list {
    color: rgb(var(--theme-text));
  }

  :global(.code-project-action-menu) {
    width: 12.25rem;
    border: 1px solid rgb(var(--theme-border) / 0.28);
    border-radius: var(--theme-container-radius);
    background: rgb(var(--theme-card) / 0.98);
    padding: 0.25rem;
    box-shadow: 0 0.5rem 1.5rem rgb(var(--theme-shadow) / 0.2);
  }

  :global(.code-project-action-menu .code-project-menu-item) {
    display: flex;
    width: 100%;
    min-height: 1.875rem;
    align-items: center;
    gap: 0.5rem;
    border-radius: var(--theme-control-radius);
    padding: 0.35rem 0.5rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.75rem;
    line-height: 1.2;
    text-align: left;
  }

  :global(.code-project-action-menu .code-project-menu-item:hover),
  :global(.code-project-action-menu .code-project-menu-item:focus-visible) {
    background: rgb(var(--theme-card-hover) / 0.68);
    color: rgb(var(--theme-text));
  }

  :global(.code-project-action-menu .code-project-menu-item:disabled) {
    opacity: 0.42;
  }

  :global(.code-project-action-menu .code-project-menu-item svg) {
    flex: 0 0 auto;
    color: rgb(var(--theme-text-tertiary));
  }


  .code-project-section-label {
    padding: 0.25rem 0.875rem 0.375rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
    font-weight: 500;
  }

  .code-project-group + .code-project-group {
    margin-top: 0.125rem;
  }

  .code-project-heading {
    display: flex;
    min-width: 0;
    align-items: center;
    padding: 0 0.5rem 0 0.375rem;
  }

  .code-project-toggle {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    align-items: center;
    gap: 0.375rem;
    border-radius: var(--theme-control-radius);
    padding: 0.4375rem 0.25rem;
    color: rgb(var(--theme-text));
    text-align: left;
  }

  .code-project-toggle:hover,
  .code-project-toggle:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.58);
  }

  .code-project-toggle :global(.code-project-chevron) {
    flex: 0 0 auto;
    color: rgb(var(--theme-text-tertiary));
  }

  .code-project-toggle :global(.code-project-folder) {
    flex: 0 0 auto;
    color: rgb(var(--theme-text-secondary));
  }

  .code-project-name {
    min-width: 0;
    flex: 1 1 auto;
    overflow: hidden;
    font-size: 0.75rem;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .code-project-storage {
    max-width: 6.5rem;
    flex: 0 1 auto;
    overflow: hidden;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.5625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .code-project-storage--missing {
    color: rgb(var(--theme-warning));
  }

  .code-project-more,
  .code-thread-more {
    display: grid;
    flex: 0 0 auto;
    place-items: center;
    border-radius: var(--theme-control-radius);
    color: rgb(var(--theme-text-tertiary));
    opacity: 0;
  }

  .code-project-more {
    width: 1.625rem;
    height: 1.625rem;
  }

  .code-thread-more {
    width: 1.5rem;
    height: 1.5rem;
  }

  .code-project-heading:hover .code-project-more,
  .code-project-more:focus-visible,
  .code-project-more--open,
  .code-thread-row:hover .code-thread-more,
  .code-thread-more:focus-visible,
  .code-thread-more--open {
    opacity: 1;
  }

  .code-project-more:hover,
  .code-project-more:focus-visible,
  .code-project-more--open,
  .code-thread-more:hover,
  .code-thread-more:focus-visible,
  .code-thread-more--open {
    background: rgb(var(--theme-card-hover) / 0.72);
    color: rgb(var(--theme-text));
  }

  .code-project-threads {
    padding: 0 0.5rem 0.25rem 1.625rem;
  }

  .code-thread-row {
    display: flex;
    min-width: 0;
    align-items: center;
    border-radius: var(--theme-control-radius);
  }

  .code-thread-row:hover,
  .code-thread-row:focus-within {
    background: rgb(var(--theme-card-hover) / 0.55);
  }

  .code-thread-row--selected {
    background: rgb(var(--theme-card-hover) / 0.82);
  }

  .code-thread-open {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    align-items: center;
    gap: 0.4375rem;
    padding: 0.375rem 0.25rem 0.375rem 0.5rem;
    text-align: left;
  }

  .code-thread-open :global(.is-active) {
    flex: 0 0 auto;
    color: rgb(var(--theme-link));
  }

  .code-thread-open :global(.is-finished) {
    flex: 0 0 auto;
    color: rgb(var(--theme-text-faint));
  }

  .code-thread-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    align-items: baseline;
    gap: 0.5rem;
  }

  .code-thread-title {
    min-width: 0;
    flex: 1 1 auto;
    overflow: hidden;
    color: rgb(var(--theme-text));
    font-size: 0.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .code-thread-status {
    max-width: 5.5rem;
    flex: 0 1 auto;
    overflow: hidden;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.5625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .code-project-show-more {
    width: 100%;
    border-radius: var(--theme-control-radius);
    padding: 0.3rem 0.5rem 0.3rem 1.0625rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
    text-align: left;
  }

  .code-project-show-more:hover,
  .code-project-show-more:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.48);
    color: rgb(var(--theme-text-secondary));
  }

  .code-archived-heading {
    display: flex;
    width: calc(100% - 1rem);
    align-items: center;
    gap: 0.375rem;
    margin: 0.75rem 0.5rem 0.25rem;
    border-top: 1px solid rgb(var(--theme-border) / 0.24);
    padding: 0.625rem 0.375rem 0.25rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
    font-weight: 500;
    text-align: left;
  }

  .code-archived-heading:hover,
  .code-archived-heading:focus-visible {
    color: rgb(var(--theme-text-secondary));
  }

  .code-archived-count {
    display: grid;
    min-width: 1rem;
    height: 1rem;
    place-items: center;
    border-radius: 9999px;
    background: rgb(var(--theme-pane-muted) / 0.72);
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.5625rem;
  }

</style>
