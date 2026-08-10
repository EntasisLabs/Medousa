<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    Archive,
    ArchiveRestore,
    ArrowUp,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    CircleDot,
    Cloud,
    Code2,
    Download,
    FilePlus2,
    Folder,
    FolderOpen,
    FolderPlus,
    GitPullRequestArrow,
    GitBranch,
    HardDriveDownload,
    Laptop,
    MoreHorizontal,
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
    setForgeRepositoryArchived,
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
  import { closeUndertaking } from "$lib/utils/undertakingWorkspace";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import { pickExternalFolder, rootLabelFromPath } from "$lib/utils/externalDeskApi";
  import { revealFileInFinder } from "$lib/utils/vaultFilesystem";
  import {
    placeDockPopover,
    type DockPopoverPlacement,
  } from "$lib/utils/dockPopoverPlace";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
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
  let expandedProjects = $state<Record<string, boolean>>({});
  let expandedThreadLists = $state<Record<string, boolean>>({});
  let archivedExpanded = $state(false);
  let projectMenuOpen = $state<string | null>(null);
  let threadMenuOpen = $state<string | null>(null);
  let branchPickerOpen = $state(false);
  let branchPickerStep = $state<"source" | "local" | "remotes" | "remote" | "revision">("source");
  let branchQuery = $state("");
  let selectedRemote = $state<string | null>(null);
  let manualRevision = $state("");

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

  const recentRepositories = $derived(repositoryCatalog.filter((entry) => entry.available));
  const repositoryReady = $derived(
    Boolean(repository && repository.has_commits !== false && baseRef.trim()),
  );
  const localBranches = $derived.by(() => {
    const branches = repository?.local_branches?.filter(Boolean) ?? [];
    if (branches.length > 0) return branches;
    return [...new Set(
      [repository?.current_branch, repository?.suggested_base_ref].filter(
        (value): value is string => Boolean(value?.trim()),
      ),
    )];
  });
  const remoteBranchGroups = $derived(repository?.remote_branches ?? []);
  const selectedRemoteGroup = $derived(
    remoteBranchGroups.find((remote) => remote.name === selectedRemote) ?? null,
  );
  const selectedBranchSource = $derived.by(() => {
    for (const remote of remoteBranchGroups) {
      const prefix = `${remote.name}/`;
      if (baseRef.startsWith(prefix)) {
        return {
          kind: "remote" as const,
          remote: remote.name,
          branch: baseRef.slice(prefix.length),
        };
      }
    }
    return { kind: "local" as const, remote: null, branch: baseRef };
  });
  const branchSelectionLabel = $derived(
    selectedBranchSource.kind === "remote"
      ? `${selectedBranchSource.remote} / ${selectedBranchSource.branch}`
      : `Local / ${selectedBranchSource.branch || "Choose branch"}`,
  );

  function filteredBranches(branches: string[]): string[] {
    const query = branchQuery.trim().toLocaleLowerCase();
    if (!query) return branches;
    return branches.filter((branch) => branch.toLocaleLowerCase().includes(query));
  }

  function openBranchPicker(open: boolean) {
    branchPickerOpen = open;
    if (!open) return;
    branchPickerStep = "source";
    branchQuery = "";
    selectedRemote = selectedBranchSource.remote;
    manualRevision = baseRef;
  }

  function chooseLocalSource() {
    branchPickerStep = "local";
    branchQuery = "";
  }

  function chooseRemoteSource() {
    branchQuery = "";
    if (remoteBranchGroups.length === 1) {
      selectedRemote = remoteBranchGroups[0].name;
      branchPickerStep = "remote";
      return;
    }
    branchPickerStep = "remotes";
  }

  function chooseRemote(remote: string) {
    selectedRemote = remote;
    branchPickerStep = "remote";
    branchQuery = "";
  }

  function chooseBaseRef(reference: string) {
    baseRef = reference;
    branchPickerOpen = false;
    branchQuery = "";
  }

  function branchPickerBack() {
    branchQuery = "";
    if (branchPickerStep === "remote" && remoteBranchGroups.length > 1) {
      branchPickerStep = "remotes";
      return;
    }
    branchPickerStep = "source";
  }

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
      branchPickerOpen = false;
      branchPickerStep = "source";
      branchQuery = "";
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
      branchPickerOpen = false;
      branchPickerStep = "source";
      branchQuery = "";
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

  function handleOutcomeKeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" || (!event.metaKey && !event.ctrlKey)) return;
    event.preventDefault();
    void create();
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
      class="code-create flex min-h-0 flex-1 flex-col"
      onsubmit={(event) => {
        event.preventDefault();
        void create();
      }}
    >
      <div class="code-create-header flex items-center justify-between px-3 py-2.5">
        <div class="min-w-0">
          <p class="text-sm font-medium text-surface-100">New change</p>
          <p class="truncate text-xs text-content-quiet">
            {repository ? `In ${repository.display_name}` : "Choose a repository to begin"}
          </p>
        </div>
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Cancel new change"
          title="Cancel"
          onclick={() => {
            creating = false;
            repository = null;
            duplicateAcknowledged = false;
          }}
        ><X size={14} strokeWidth={1.75} /></button>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto p-2.5">
        {#if repository}
          <section class="code-create-repository">
            <div class="flex min-w-0 items-start gap-2.5">
              <span class="code-create-repository-icon"><Code2 size={15} strokeWidth={1.75} /></span>
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm font-medium text-surface-100">{repository.display_name}</p>
                <p class="mt-0.5 truncate font-mono text-xs text-content-quiet">{repository.path}</p>
                {#if repository.dirty}
                  <p class="code-create-repository-status" title={repository.state_explanation}>
                    <CircleDot size={9} strokeWidth={1.75} />
                    {repository.changed_files} local {repository.changed_files === 1 ? "change" : "changes"}
                  </p>
                {/if}
              </div>
              <button type="button" class="code-create-text-action" onclick={() => {
                repository = null;
                duplicateAcknowledged = false;
              }}>Change</button>
            </div>
          </section>

          {#if repository.existing_projects.length > 0 && !duplicateAcknowledged}
            <section class="mt-3">
              <div class="px-1">
                <p class="text-xs font-medium text-surface-200">Active changes</p>
                <p class="mt-0.5 text-xs leading-relaxed text-content-quiet">Continue one, or create a separate working copy.</p>
              </div>
              <div class="code-create-list mt-1.5 overflow-hidden">
                {#each repository.existing_projects.slice(0, 4) as existing (existing.id)}
                  <button type="button" class="code-create-existing" onclick={() => void openItem(existing.id, existing.title)}>
                    <CircleDot size={13} strokeWidth={1.75} class="shrink-0 text-primary-300" />
                    <span class="min-w-0 flex-1 truncate text-xs font-medium text-surface-200">{existing.title}</span>
                    <span class="shrink-0 text-xs text-content-quiet">{humanPhaseLabel(existing.human_phase)}</span>
                  </button>
                {/each}
              </div>
              <button type="button" class="code-create-new-separate" onclick={() => (duplicateAcknowledged = true)}>
                <Plus size={13} strokeWidth={1.75} />
                New change in a separate worktree
              </button>
            </section>
          {:else}
            <section class="mt-3">
              <label for="code-change-outcome" class="px-1 text-xs font-medium text-surface-200">What do you want changed?</label>
              <div class="code-create-prompt mt-1.5">
                <textarea
                  id="code-change-outcome"
                  class="min-h-24 w-full resize-none border-0 bg-transparent text-sm text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
                  placeholder="Make indexing cancellation-safe…"
                  bind:value={outcome}
                  onkeydown={handleOutcomeKeydown}
                ></textarea>
                <div class="code-create-prompt-toolbar mt-2 flex min-w-0 items-center gap-2 pt-2">
                  <OverflowMenu
                    class="min-w-0 flex-1"
                    open={branchPickerOpen}
                    onOpenChange={openBranchPicker}
                    align="left"
                    panelWidth={280}
                    panelClass="code-branch-picker"
                    label="Choose starting branch"
                  >
                    {#snippet trigger({ open, toggle })}
                      <button
                        type="button"
                        class="code-branch-trigger"
                        class:code-branch-trigger--open={open}
                        aria-label={`Start from ${branchSelectionLabel}`}
                        title="Choose starting branch"
                        aria-haspopup="menu"
                        aria-expanded={open}
                        onclick={toggle}
                      >
                        <GitBranch size={12} strokeWidth={1.75} />
                        <span class="code-branch-trigger-prefix">Start from</span>
                        <span class="code-branch-trigger-value">{branchSelectionLabel}</span>
                        <ChevronDown size={11} strokeWidth={1.75} />
                      </button>
                    {/snippet}

                    {#if branchPickerStep === "source"}
                      <p class="code-branch-picker-title">Choose source</p>
                      <button type="button" role="menuitem" class="code-branch-source" onclick={chooseLocalSource}>
                        <span class="code-branch-source-icon"><Laptop size={14} strokeWidth={1.75} /></span>
                        <span class="min-w-0 flex-1">
                          <span class="block">Local</span>
                          <span class="code-branch-source-detail">Branches on this computer</span>
                        </span>
                        <ChevronRight size={12} strokeWidth={1.75} />
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        class="code-branch-source"
                        disabled={remoteBranchGroups.length === 0}
                        onclick={chooseRemoteSource}
                      >
                        <span class="code-branch-source-icon"><Cloud size={14} strokeWidth={1.75} /></span>
                        <span class="min-w-0 flex-1">
                          <span class="block">Remote</span>
                          <span class="code-branch-source-detail">
                            {remoteBranchGroups.length === 0
                              ? "No remote branches"
                              : `${remoteBranchGroups.length} ${remoteBranchGroups.length === 1 ? "remote" : "remotes"}`}
                          </span>
                        </span>
                        <ChevronRight size={12} strokeWidth={1.75} />
                      </button>
                      <button type="button" role="menuitem" class="code-branch-revision-link" onclick={() => {
                        branchPickerStep = "revision";
                        manualRevision = baseRef;
                      }}>Enter a branch or commit…</button>
                    {:else if branchPickerStep === "remotes"}
                      <button type="button" class="code-branch-picker-back" onclick={branchPickerBack}>
                        <ChevronLeft size={12} strokeWidth={1.75} />
                        <span>Start from / Remote</span>
                      </button>
                      <div class="code-branch-list">
                        {#each remoteBranchGroups as remote (remote.name)}
                          <button type="button" role="menuitem" class="code-branch-list-row" onclick={() => chooseRemote(remote.name)}>
                            <Cloud size={13} strokeWidth={1.75} />
                            <span class="min-w-0 flex-1 truncate">{remote.name}</span>
                            <span class="code-branch-row-meta">{remote.branches.length}</span>
                            <ChevronRight size={11} strokeWidth={1.75} />
                          </button>
                        {/each}
                      </div>
                    {:else if branchPickerStep === "local"}
                      <button type="button" class="code-branch-picker-back" onclick={branchPickerBack}>
                        <ChevronLeft size={12} strokeWidth={1.75} />
                        <span>Start from / Local</span>
                      </button>
                      <label class="code-branch-search">
                        <Search size={12} strokeWidth={1.75} />
                        <input aria-label="Search local branches" placeholder="Search local branches…" bind:value={branchQuery} />
                      </label>
                      <div class="code-branch-list">
                        {#each filteredBranches(localBranches) as branch (branch)}
                          <button type="button" role="menuitem" class="code-branch-list-row" onclick={() => chooseBaseRef(branch)}>
                            <span class="code-branch-check">{#if baseRef === branch}<Check size={12} strokeWidth={2} />{/if}</span>
                            <span class="min-w-0 flex-1 truncate">{branch}</span>
                            {#if repository.current_branch === branch}<span class="code-branch-row-meta">current</span>{/if}
                          </button>
                        {:else}
                          <p class="code-branch-empty">No matching local branches</p>
                        {/each}
                      </div>
                    {:else if branchPickerStep === "remote" && selectedRemoteGroup}
                      <button type="button" class="code-branch-picker-back" onclick={branchPickerBack}>
                        <ChevronLeft size={12} strokeWidth={1.75} />
                        <span>Remote / {selectedRemoteGroup.name}</span>
                      </button>
                      <label class="code-branch-search">
                        <Search size={12} strokeWidth={1.75} />
                        <input aria-label={`Search ${selectedRemoteGroup.name} branches`} placeholder={`Search ${selectedRemoteGroup.name} branches…`} bind:value={branchQuery} />
                      </label>
                      <div class="code-branch-list">
                        {#each filteredBranches(selectedRemoteGroup.branches) as branch (branch)}
                          {@const reference = `${selectedRemoteGroup.name}/${branch}`}
                          <button type="button" role="menuitem" class="code-branch-list-row" onclick={() => chooseBaseRef(reference)}>
                            <span class="code-branch-check">{#if baseRef === reference}<Check size={12} strokeWidth={2} />{/if}</span>
                            <span class="min-w-0 flex-1 truncate">{branch}</span>
                            {#if selectedRemoteGroup.default_branch === branch}<span class="code-branch-row-meta">default</span>{/if}
                          </button>
                        {:else}
                          <p class="code-branch-empty">No matching remote branches</p>
                        {/each}
                      </div>
                    {:else if branchPickerStep === "revision"}
                      <button type="button" class="code-branch-picker-back" onclick={branchPickerBack}>
                        <ChevronLeft size={12} strokeWidth={1.75} />
                        <span>Enter a revision</span>
                      </button>
                      <div class="code-branch-revision">
                        <input
                          aria-label="Branch, tag, or commit"
                          placeholder="Branch, tag, or commit"
                          bind:value={manualRevision}
                          onkeydown={(event) => {
                            if (event.key !== "Enter" || !manualRevision.trim()) return;
                            event.preventDefault();
                            chooseBaseRef(manualRevision.trim());
                          }}
                        />
                        <button type="button" disabled={!manualRevision.trim()} onclick={() => chooseBaseRef(manualRevision.trim())}>Use</button>
                      </div>
                    {/if}
                  </OverflowMenu>
                  <button
                    type="submit"
                    class="code-create-submit"
                    disabled={busy || inspecting || !outcome.trim() || !repositoryReady}
                    aria-label={busy ? "Preparing change" : "Start change"}
                    title="Start change (⌘Enter)"
                  ><ArrowUp size={15} strokeWidth={2} /></button>
                </div>
              </div>
            </section>
          {/if}
        {:else}
          <div class="grid gap-1">
            <p class="px-1 pb-1 text-xs font-medium text-content-tertiary">Open repository</p>
          {#if coLocated}
            <button type="button" class="code-create-source" onclick={() => void pickRepository()}>
              <FolderOpen size={15} class="text-content-link" />
              <span class="min-w-0 flex-1 text-left">
                <span class="block text-sm font-medium text-surface-100">Choose a folder…</span>
                <span class="mt-0.5 block text-xs text-content-quiet">Open a repository on this computer</span>
              </span>
            </button>
            {#if currentFolder}
              <button type="button" class="code-create-row" onclick={() => void chooseRepository(currentFolder.path)}>
                <FolderOpen size={13} class="shrink-0" /><span class="min-w-0 flex-1 truncate">Current folder · {currentFolder.label}</span>
              </button>
            {/if}
          {:else}
            <button type="button" class="code-create-source" onclick={() => void browseRepositoryFolder(null, "repository")}>
              <FolderOpen size={15} class="text-content-link" />
              <span class="min-w-0 flex-1 text-left">
                <span class="block text-sm font-medium text-surface-100">Browse connected computer…</span>
                <span class="mt-0.5 block text-xs text-content-quiet">Open a repository on the workshop</span>
              </span>
            </button>
          {/if}
          <button type="button" class="code-create-row" onclick={() => void openHostedRepository()}>
            <Download size={13} class="shrink-0" />
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
          {#if recentRepositories.length}
            <p class="px-1 pb-1 pt-3 text-xs font-medium text-content-tertiary">Recent</p>
            <div class="code-create-list overflow-hidden">
              {#each recentRepositories.slice(0, 8) as recent (recent.path)}
                <div class="code-create-recent group">
                  <button type="button" class="flex min-w-0 flex-1 items-center gap-2.5 text-left" onclick={() => void chooseRepository(recent.path)}>
                    <Code2 size={14} class="shrink-0 text-content-tertiary" />
                    <span class="min-w-0 flex-1">
                      <span class="block truncate text-xs font-medium text-surface-200">{recent.display_name}</span>
                      <span class="mt-0.5 block truncate text-xs text-content-quiet">{recent.has_commits === false ? "No commits" : recent.current_branch ?? recent.suggested_base_ref ?? "Repository"}</span>
                    </span>
                  </button>
                  <button
                    type="button"
                    class="shrink-0 rounded-md p-1 {recent.pinned ? 'text-content-link' : 'text-content-faint opacity-0 group-hover:opacity-100'}"
                    aria-label={recent.pinned ? `Unpin ${recent.display_name}` : `Pin ${recent.display_name}`}
                    onclick={(event) => void togglePinned(recent, event)}
                  ><Pin size={12} fill={recent.pinned ? "currentColor" : "none"} /></button>
                </div>
              {/each}
            </div>
          {/if}
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
      </div>
    </form>
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
                void chooseRepository(group.path!);
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

  :global(.code-branch-picker) {
    width: 17.5rem;
    border: 1px solid rgb(var(--theme-border) / 0.28);
    border-radius: var(--theme-container-radius);
    background: rgb(var(--theme-card) / 0.99);
    padding: 0.3rem;
    box-shadow: 0 0.5rem 1.5rem rgb(var(--theme-shadow) / 0.2);
  }

  .code-branch-trigger {
    display: flex;
    width: 100%;
    min-width: 0;
    align-items: center;
    gap: 0.35rem;
    border-radius: var(--theme-control-radius);
    padding: 0.25rem 0.35rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
    text-align: left;
  }

  .code-branch-trigger:hover,
  .code-branch-trigger:focus-visible,
  .code-branch-trigger--open {
    background: rgb(var(--theme-card-hover) / 0.58);
    color: rgb(var(--theme-text-secondary));
  }

  .code-branch-trigger-prefix {
    flex: 0 0 auto;
    color: rgb(var(--theme-text-tertiary));
  }

  .code-branch-trigger-value {
    min-width: 0;
    overflow: hidden;
    color: rgb(var(--theme-text-secondary));
    font-family: var(--font-mono, ui-monospace, monospace);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .code-branch-picker-title {
    padding: 0.4rem 0.5rem 0.3rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
    font-weight: 500;
  }

  .code-branch-source,
  .code-branch-list-row {
    display: flex;
    width: 100%;
    min-width: 0;
    align-items: center;
    border-radius: var(--theme-control-radius);
    color: rgb(var(--theme-text-secondary));
    text-align: left;
  }

  .code-branch-source {
    gap: 0.55rem;
    padding: 0.5rem;
    font-size: 0.75rem;
  }

  .code-branch-source:hover,
  .code-branch-source:focus-visible,
  .code-branch-list-row:hover,
  .code-branch-list-row:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.68);
    color: rgb(var(--theme-text));
  }

  .code-branch-source:disabled {
    opacity: 0.44;
  }

  .code-branch-source-icon {
    display: grid;
    width: 1.625rem;
    height: 1.625rem;
    flex: 0 0 auto;
    place-items: center;
    border-radius: var(--theme-control-radius);
    background: rgb(var(--theme-pane-muted) / 0.7);
    color: rgb(var(--theme-text-secondary));
  }

  .code-branch-source-detail {
    display: block;
    margin-top: 0.08rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.59375rem;
  }

  .code-branch-revision-link {
    width: 100%;
    margin-top: 0.2rem;
    border-top: 1px solid rgb(var(--theme-border) / 0.2);
    padding: 0.45rem 0.5rem 0.3rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
    text-align: left;
  }

  .code-branch-revision-link:hover,
  .code-branch-revision-link:focus-visible {
    color: rgb(var(--theme-text-secondary));
  }

  .code-branch-picker-back {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.35rem;
    border-radius: var(--theme-control-radius);
    padding: 0.4rem 0.45rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.6875rem;
    font-weight: 500;
    text-align: left;
  }

  .code-branch-picker-back:hover,
  .code-branch-picker-back:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.58);
    color: rgb(var(--theme-text));
  }

  .code-branch-search {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0.2rem 0.25rem 0.3rem;
    border: 1px solid rgb(var(--theme-border) / 0.3);
    border-radius: var(--theme-control-radius);
    padding: 0.35rem 0.45rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .code-branch-search:focus-within {
    border-color: rgb(var(--theme-focus) / 0.56);
  }

  .code-branch-search input {
    min-width: 0;
    flex: 1 1 auto;
    border: 0;
    background: transparent;
    color: rgb(var(--theme-text));
    font-size: 0.6875rem;
    outline: 0;
  }

  .code-branch-search input::placeholder {
    color: rgb(var(--theme-text-tertiary));
  }

  .code-branch-list {
    max-height: 14rem;
    overflow-y: auto;
  }

  .code-branch-list-row {
    gap: 0.45rem;
    min-height: 1.875rem;
    padding: 0.35rem 0.5rem;
    font-size: 0.75rem;
  }

  .code-branch-check {
    display: grid;
    width: 0.875rem;
    flex: 0 0 auto;
    place-items: center;
    color: rgb(var(--theme-link));
  }

  .code-branch-row-meta {
    flex: 0 0 auto;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.5625rem;
  }

  .code-branch-empty {
    padding: 0.75rem 0.5rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
    text-align: center;
  }

  .code-branch-revision {
    display: flex;
    gap: 0.35rem;
    padding: 0.35rem 0.25rem 0.25rem;
  }

  .code-branch-revision input {
    min-width: 0;
    flex: 1 1 auto;
    border: 1px solid rgb(var(--theme-border) / 0.3);
    border-radius: var(--theme-control-radius);
    background: rgb(var(--theme-pane) / 0.42);
    padding: 0.4rem 0.5rem;
    color: rgb(var(--theme-text));
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.6875rem;
    outline: 0;
  }

  .code-branch-revision input:focus {
    border-color: rgb(var(--theme-focus) / 0.56);
  }

  .code-branch-revision button {
    border-radius: var(--theme-control-radius);
    background: rgb(var(--theme-action));
    padding: 0.4rem 0.6rem;
    color: rgb(var(--on-primary));
    font-size: 0.6875rem;
  }

  .code-branch-revision button:disabled {
    opacity: 0.36;
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

  .code-field {
    width: 100%;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    border-radius: 0.25rem;
    background: rgb(var(--color-surface-900));
    padding: 0.3rem 0.45rem;
    font-size: 0.7rem;
  }

  .code-create {
    background: rgb(var(--theme-pane) / 0.24);
  }

  .code-create-header {
    border-bottom: 1px solid rgb(var(--theme-border) / 0.28);
  }

  .code-create-repository,
  .code-create-prompt,
  .code-create-source,
  .code-create-list {
    border: 1px solid rgb(var(--theme-border) / 0.34);
    border-radius: var(--theme-container-radius);
    background: rgb(var(--theme-card) / 0.52);
  }

  .code-create-repository {
    padding: 0.75rem;
  }

  .code-create-repository-status {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    margin-top: 0.4rem;
    color: rgb(var(--theme-warning));
    font-size: 0.625rem;
  }

  .code-create-repository-icon {
    display: grid;
    width: 1.75rem;
    height: 1.75rem;
    flex: 0 0 auto;
    place-items: center;
    border-radius: var(--theme-control-radius);
    background: rgb(var(--theme-pane-muted) / 0.78);
    color: rgb(var(--theme-text-secondary));
  }

  .code-create-text-action {
    flex: 0 0 auto;
    border-radius: var(--theme-control-radius);
    padding: 0.25rem 0.375rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.75rem;
  }

  .code-create-text-action:hover,
  .code-create-text-action:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.72);
    color: rgb(var(--theme-text));
  }

  .code-create-existing,
  .code-create-recent {
    display: flex;
    width: 100%;
    min-width: 0;
    align-items: center;
    gap: 0.625rem;
    padding: 0.625rem 0.75rem;
    text-align: left;
  }

  .code-create-existing + .code-create-existing,
  .code-create-recent + .code-create-recent {
    border-top: 1px solid rgb(var(--theme-border) / 0.22);
  }

  .code-create-existing:hover,
  .code-create-existing:focus-visible,
  .code-create-recent:hover,
  .code-create-recent:focus-within {
    background: rgb(var(--theme-card-hover) / 0.62);
  }

  .code-create-new-separate,
  .code-create-row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.5rem;
    border-radius: var(--theme-control-radius);
    padding: 0.5rem 0.625rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.75rem;
    text-align: left;
  }

  .code-create-new-separate {
    margin-top: 0.375rem;
    color: rgb(var(--theme-link));
  }

  .code-create-new-separate:hover,
  .code-create-new-separate:focus-visible,
  .code-create-row:hover,
  .code-create-row:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.62);
    color: rgb(var(--theme-text));
  }

  .code-create-prompt {
    padding: 0.75rem;
  }

  .code-create-prompt-toolbar {
    border-top: 1px solid rgb(var(--theme-border) / 0.24);
  }

  .code-create-source {
    display: flex;
    width: 100%;
    align-items: flex-start;
    gap: 0.625rem;
    padding: 0.75rem;
  }

  .code-create-source:hover,
  .code-create-source:focus-visible,
  .code-create-prompt:focus-within {
    border-color: rgb(var(--theme-focus) / 0.58);
    background: rgb(var(--theme-card-hover) / 0.58);
  }

  .code-create-submit {
    display: grid;
    width: 1.75rem;
    height: 1.75rem;
    flex: 0 0 auto;
    place-items: center;
    border-radius: 9999px;
    background: rgb(var(--theme-action));
    color: rgb(var(--on-primary));
    transition: opacity 120ms ease, background-color 120ms ease;
  }

  .code-create-submit:hover:not(:disabled),
  .code-create-submit:focus-visible:not(:disabled) {
    background: rgb(var(--theme-action-hover));
  }

  .code-create-submit:disabled {
    opacity: 0.32;
  }
</style>
