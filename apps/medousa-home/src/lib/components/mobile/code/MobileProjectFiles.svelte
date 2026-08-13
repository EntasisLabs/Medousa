<script lang="ts">
  import { ChevronRight, Folder, X } from "@lucide/svelte";
  import CodeFileIcon from "$lib/components/lme/explorers/CodeFileIcon.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
  import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";
  import { ensureCodeWorkspaceTree } from "$lib/utils/codeWorkspaceController";
  import {
    buildCodeSourceTreeAsync,
    type CodeSourceTreeNode,
  } from "$lib/utils/codeSourceTree";
  import { humanizeForgeMessage, type ForgeSourceTree } from "$lib/forge";
  import { openMobileCodeFile } from "$lib/utils/mobileCodeOpen";
  import type { MobileCodeFilesFilter } from "$lib/utils/mobileCodeLanding";

  interface Props {
    workId: string;
  }

  let { workId }: Props = $props();

  let tree = $state<ForgeSourceTree | null>(null);
  let roots = $state<CodeSourceTreeNode[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let searchFocused = $state(false);
  let searchInput = $state<HTMLInputElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);

  const query = $derived(mobileCodeWorkspaceState.filesQuery);
  const directory = $derived(mobileCodeWorkspaceState.presentation?.filesDirectory ?? "");
  const hasChanged = $derived(Boolean(tree?.files.some((file) => Boolean(file.status))));
  const recentTabs = $derived(codeWorkspace.recentTabsFor(workId));
  const filter = $derived(
    mobileCodeWorkspaceState.resolvedFilesFilter({
      hasChangedFiles: hasChanged,
      hasRecentFiles: recentTabs.length > 0,
    }),
  );
  const dirtyPaths = $derived(
    new Set(
      codeWorkspace
        .tabsFor(workId)
        .filter((tab) => codeWorkspace.isDirty(tab))
        .map((tab) => tab.path),
    ),
  );
  const openPaths = $derived(new Set(codeWorkspace.tabsFor(workId).map((tab) => tab.path)));
  const crumbs = $derived(directory ? directory.split("/").filter(Boolean) : []);

  function nodeAtDirectory(nodes: CodeSourceTreeNode[], path: string): CodeSourceTreeNode[] {
    if (!path) return nodes;
    const parts = path.split("/").filter(Boolean);
    let current = nodes;
    for (const part of parts) {
      const next = current.find((node) => node.kind === "directory" && node.name === part);
      if (!next) return [];
      current = next.children;
    }
    return current;
  }

  type FileRow = {
    path: string;
    name: string;
    parent: string;
    kind: "file" | "directory";
    status?: string | null;
  };

  const rows = $derived.by((): FileRow[] => {
    const needle = query.trim().toLowerCase();
    if (needle) {
      return (tree?.files ?? [])
        .filter((file) => file.path.toLowerCase().includes(needle))
        .map((file) => ({
          path: file.path,
          name: file.path.split("/").at(-1) ?? file.path,
          parent: file.path.split("/").slice(0, -1).join("/"),
          kind: "file" as const,
          status: file.status,
        }));
    }
    if (filter === "changed") {
      return (tree?.files ?? [])
        .filter((file) => Boolean(file.status))
        .map((file) => ({
          path: file.path,
          name: file.path.split("/").at(-1) ?? file.path,
          parent: file.path.split("/").slice(0, -1).join("/"),
          kind: "file" as const,
          status: file.status,
        }));
    }
    if (filter === "recent") {
      return recentTabs.map((tab) => ({
        path: tab.path,
        name: tab.title,
        parent: tab.path.split("/").slice(0, -1).join("/"),
        kind: "file" as const,
        status: tree?.files.find((file) => file.path === tab.path)?.status,
      }));
    }
    return nodeAtDirectory(roots, directory)
      .filter((node) => !node.name.startsWith("."))
      .map((node) => ({
        path: node.path,
        name: node.name,
        parent: directory,
        kind: node.kind,
        status: node.status,
      }));
  });

  async function loadTree() {
    loading = true;
    error = null;
    try {
      const next = await ensureCodeWorkspaceTree(workId, { force: true });
      tree = next;
      roots = await buildCodeSourceTreeAsync(next.files);
    } catch (err) {
      tree = null;
      roots = [];
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void workId;
    void loadTree();
  });

  $effect(() => {
    const onSearch = () => {
      searchFocused = true;
      queueMicrotask(() => searchInput?.focus());
    };
    window.addEventListener("medousa-mobile-code-search", onSearch);
    return () => window.removeEventListener("medousa-mobile-code-search", onSearch);
  });

  $effect(() => {
    if (!searchFocused && !mobileCodeWorkspaceState.ancestorSheetOpen) return;
    return registerMobileBackHandler(() => {
      if (mobileCodeWorkspaceState.ancestorSheetOpen) {
        mobileCodeWorkspaceState.ancestorSheetOpen = false;
        return true;
      }
      searchFocused = false;
      searchInput?.blur();
      return true;
    });
  });

  $effect(() => {
    const path = mobileCodeWorkspaceState.presentation?.lastOpenedPath;
    if (!path || !listEl) return;
    const row = listEl.querySelector(`[data-path="${path.replace(/"/g, "")}"]`);
    row?.scrollIntoView({ block: "center" });
  });

  function setFilter(next: MobileCodeFilesFilter) {
    haptic("light");
    mobileCodeWorkspaceState.setFilesFilter(next);
  }

  function openAncestor(index: number) {
    haptic("light");
    const next = crumbs.slice(0, index + 1).join("/");
    mobileCodeWorkspaceState.setFilesDirectory(next);
    mobileCodeWorkspaceState.ancestorSheetOpen = false;
  }

  async function openRow(row: FileRow) {
    haptic("light");
    if (row.kind === "directory") {
      mobileCodeWorkspaceState.setFilesDirectory(row.path);
      return;
    }
    await openMobileCodeFile(workId, row.path, { origin: "files" });
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="flex shrink-0 items-center gap-1 border-b border-surface-500/25 px-2 py-1.5">
    <button
      type="button"
      class="rounded-full px-2.5 py-1 text-[11px] {filter === 'changed' ? 'bg-surface-800 text-content-link' : 'text-content-quiet'}"
      onclick={() => setFilter("changed")}
    >Changed</button>
    <button
      type="button"
      class="rounded-full px-2.5 py-1 text-[11px] {filter === 'recent' ? 'bg-surface-800 text-content-link' : 'text-content-quiet'}"
      onclick={() => setFilter("recent")}
    >Recent</button>
    <button
      type="button"
      class="rounded-full px-2.5 py-1 text-[11px] {filter === 'tree' ? 'bg-surface-800 text-content-link' : 'text-content-quiet'}"
      onclick={() => setFilter("tree")}
    >Tree</button>
  </div>

  {#if filter === "tree" && directory}
    <button
      type="button"
      class="flex h-11 shrink-0 items-center gap-1 border-b border-surface-500/20 px-3 text-left text-[13px] text-content-secondary"
      onclick={() => {
        haptic("light");
        mobileCodeWorkspaceState.ancestorSheetOpen = true;
      }}
    >
      <span class="truncate font-mono">{directory}</span>
      <ChevronRight size={14} class="shrink-0 text-content-quiet" />
    </button>
  {/if}

  {#if searchFocused || query}
    <div class="flex shrink-0 items-center gap-2 border-b border-surface-500/20 px-3 py-2">
      <input
        bind:this={searchInput}
        class="min-w-0 flex-1 bg-transparent text-sm text-content-secondary outline-none"
        placeholder="Search files"
        value={query}
        oninput={(event) => {
          mobileCodeWorkspaceState.filesQuery = event.currentTarget.value;
        }}
      />
      <button
        type="button"
        class="mobile-icon-btn text-content-quiet"
        aria-label="Clear search"
        onclick={() => {
          mobileCodeWorkspaceState.filesQuery = "";
          searchFocused = false;
        }}
      >
        <X size={16} />
      </button>
    </div>
  {/if}

  {#if error}
    <p class="m-3 rounded border border-amber-500/35 bg-amber-950/25 px-3 py-2 text-[12px] text-amber-100">
      {error}
    </p>
  {/if}

  <div bind:this={listEl} class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto">
    {#if loading && rows.length === 0}
      <p class="px-4 py-6 text-sm text-content-quiet">Loading files…</p>
    {:else if rows.length === 0}
      <p class="px-4 py-6 text-sm text-content-quiet">
        {filter === "changed" ? "No changed files." : filter === "recent" ? "No recent files yet." : "This folder is empty."}
      </p>
    {:else}
      {#each rows as row (row.path)}
        <button
          type="button"
          class="flex min-h-11 w-full items-center gap-3 px-3 text-left active:bg-surface-800"
          data-path={row.path}
          onclick={() => void openRow(row)}
        >
          {#if row.kind === "directory"}
            <Folder size={16} class="shrink-0 text-content-quiet" />
          {:else}
            <CodeFileIcon path={row.path} size={16} />
          {/if}
          <span class="min-w-0 flex-1">
            <span class="flex items-center gap-2">
              <span class="truncate text-sm text-content-secondary">{row.name}</span>
              {#if dirtyPaths.has(row.path)}
                <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-300" title="Unsaved"></span>
              {:else if openPaths.has(row.path)}
                <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-content-link/80" title="Open"></span>
              {/if}
            </span>
            {#if row.parent && (filter !== "tree" || query)}
              <span class="block truncate text-[11px] text-content-quiet">{row.parent}</span>
            {/if}
          </span>
          {#if row.status}
            <span class="shrink-0 font-mono text-[10px] text-amber-200/90">{row.status}</span>
          {/if}
        </button>
      {/each}
    {/if}
  </div>
</div>

{#if mobileCodeWorkspaceState.ancestorSheetOpen}
  <div class="mobile-sheet-backdrop" role="presentation">
    <div class="mobile-sheet" role="dialog" aria-label="Ancestors" tabindex="-1">
      <div class="mobile-sheet-header">
        <p class="text-sm font-medium">Go to folder</p>
        <button
          type="button"
          class="mobile-icon-btn"
          aria-label="Close"
          onclick={() => {
            mobileCodeWorkspaceState.ancestorSheetOpen = false;
          }}
        ><X size={16} /></button>
      </div>
      <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto p-2">
        <button
          type="button"
          class="vault-menu-item w-full rounded-lg"
          onclick={() => {
            mobileCodeWorkspaceState.setFilesDirectory("");
            mobileCodeWorkspaceState.ancestorSheetOpen = false;
          }}
        >Project files</button>
        {#each crumbs as crumb, index (crumb + index)}
          <button
            type="button"
            class="vault-menu-item w-full rounded-lg"
            onclick={() => openAncestor(index)}
          >{crumbs.slice(0, index + 1).join("/")}</button>
        {/each}
      </div>
    </div>
  </div>
{/if}
