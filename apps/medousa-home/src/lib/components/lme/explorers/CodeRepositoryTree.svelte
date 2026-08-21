<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { tick, untrack } from "svelte";
  import {
    ChevronDown,
    ChevronRight,
    FilePlus2,
    FolderPlus,
    RefreshCw,
    Search,
    Pencil,
    Trash2,
    Undo2,
  } from "@lucide/svelte";
  import CodeFileIcon from "$lib/components/lme/explorers/CodeFileIcon.svelte";
  import {
    getUndertakingSource,
    createUndertakingSource,
    renameUndertakingSource,
    deleteUndertakingSource,
    applyUndertakingSourceWorkspaceEdit,
    canStartHumanEditing,
    startHumanEditingSession,
    humanizeForgeMessage,
    searchUndertakingSource,
    saveUndertakingSource,
    type ForgeSourceFile,
    type ForgeSourceSearch,
    type ForgeSourceTree,
  } from "$lib/forge";
  import { openUndertakingLocation } from "$lib/utils/undertakingLocation";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import {
    buildCodeSourceTreeAsync,
    flattenCodeSourceTree,
    CODE_TREE_MAX_VISIBLE_ROWS,
    type CodeSourceTreeNode,
  } from "$lib/utils/codeSourceTree";
  import {
    ensureCodeWorkspaceTree,
  } from "$lib/utils/codeWorkspaceController";
  import {
    traceCodeWorkspaceEnd,
    traceCodeWorkspaceStart,
  } from "$lib/utils/codeWorkspaceTrace";
  import { dispatchCodeCommand } from "$lib/commands/codeCommands";

  interface Props {
    workId: string;
    prepared: boolean;
    /** Fill remaining explorer height instead of a capped max-height. */
    fill?: boolean;
    /**
     * Find / refresh / new-file live in the parent dock (Notes grammar).
     * Tree keeps only ephemeral create/rename forms + file list.
     */
    chromeInDock?: boolean;
    query?: string;
    loading?: boolean;
  }

  let {
    workId,
    prepared,
    fill = false,
    chromeInDock = false,
    query = $bindable(""),
    loading = $bindable(false),
  }: Props = $props();
  let tree = $state<ForgeSourceTree | null>(null);
  /** Nested nodes built off the critical path — never sync-derived from 20k files. */
  let nodes = $state<CodeSourceTreeNode[]>([]);
  let building = $state(false);
  let error = $state<string | null>(null);
  let searchInput = $state<HTMLInputElement | null>(null);
  let contentSearch = $state<ForgeSourceSearch | null>(null);
  let contentSearching = $state(false);
  let creatingPath = $state(false);
  let creatingFolder = $state(false);
  let newPath = $state("");
  let mutating = $state(false);
  let selectedDirectory = $state("");
  let renamingPath = $state<string | null>(null);
  let renamingDirectory = $state<string | null>(null);
  let renameDestination = $state("");
  let deletedFile = $state<ForgeSourceFile | null>(null);
  let renameInput = $state<HTMLInputElement | null>(null);
  const expanded = new SvelteSet<string>();
  /** Monotonic id so only the latest in-flight response may update UI / clear loading. */
  let loadToken = 0;

  export function refreshFiles() {
    void load(false, true);
  }

  export function startNewFile() {
    creatingFolder = false;
    creatingPath = true;
  }

  export function startNewFolder() {
    creatingPath = false;
    creatingFolder = true;
  }

  export function focusFind() {
    searchInput?.focus();
    searchInput?.select();
  }

  const rows = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (needle) {
      const matched: Array<{
        kind: "file";
        name: string;
        path: string;
        byteSize: number;
        status: string | null | undefined;
        children: CodeSourceTreeNode[];
        depth: number;
      }> = [];
      for (const file of tree?.files ?? []) {
        if (!file.path.toLowerCase().includes(needle)) continue;
        matched.push({
          kind: "file",
          name: file.path.split("/").pop() ?? file.path,
          path: file.path,
          byteSize: file.byte_size,
          status: file.status,
          children: [],
          depth: 0,
        });
        if (matched.length >= 200) break;
      }
      return matched;
    }
    return flattenCodeSourceTree(nodes, expanded);
  });
  const visibleRows = $derived(rows.slice(0, CODE_TREE_MAX_VISIBLE_ROWS));
  const hiddenRowCount = $derived(Math.max(0, rows.length - visibleRows.length));

  async function applyTreePayload(next: ForgeSourceTree, token: number) {
    // Keep the shell interactive while we index — do not sync-build on assign.
    const trace = traceCodeWorkspaceStart("tree-index", next.work_id);
    building = true;
    tree = next;
    nodes = [];
    await new Promise<void>((resolve) => {
      if (typeof requestAnimationFrame === "function") requestAnimationFrame(() => resolve());
      else setTimeout(resolve, 0);
    });
    if (token !== loadToken) return;
    const built = await buildCodeSourceTreeAsync(
      next.files,
      () => token !== loadToken,
    );
    if (token !== loadToken) return;
    traceCodeWorkspaceEnd(trace, `${next.files.length} files`);
    nodes = built;
    // Only auto-open the top level when it is small — expanding huge dirs freezes paint.
    if (built.length > 0 && built.length <= 24) {
      for (const node of built) {
        if (node.kind === "directory" && node.children.length <= 40) {
          expanded.add(node.path);
        }
      }
    }
    building = false;
  }

  async function load(background = false, force = false) {
    if (!prepared || !workId) {
      tree = null;
      nodes = [];
      loading = false;
      building = false;
      if (!prepared) error = null;
      return;
    }
    const token = ++loadToken;
    if (!background) {
      loading = true;
      error = null;
    }
    try {
      const next = await ensureCodeWorkspaceTree(workId, { force });
      if (token !== loadToken) return;
      if (!background) error = null;
      loading = false;
      await applyTreePayload(next, token);
      if (token !== loadToken) return;
    } catch (err) {
      if (token !== loadToken) return;
      if (!background || !tree) {
        error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
      }
    } finally {
      if (token === loadToken) {
        loading = false;
        building = false;
      }
    }
  }

  async function searchContent() {
    const needle = query.trim();
    if (needle.length < 2) return;
    contentSearching = true;
    error = null;
    try {
      contentSearch = await searchUndertakingSource(workId, needle);
    } catch (err) {
      error = humanizeForgeMessage(err instanceof Error ? err.message : String(err));
    } finally {
      contentSearching = false;
    }
  }

  async function ensureLease() {
    const active = undertakings.active;
    if (
      active?.workId === workId &&
      active.leaseId &&
      active.leaseGeneration != null
    ) {
      return { lease_id: active.leaseId, generation: active.leaseGeneration };
    }
    const detail = undertakings.detail;
    if (detail?.id !== workId || !canStartHumanEditing(detail.allowed_actions)) {
      throw new Error(
        detail?.allowed_actions.continue_editing?.reason
          ?? detail?.allowed_actions.begin_attempt.reason
          ?? "This project is not ready for file changes",
      );
    }
    const begun = await startHumanEditingSession(workId, detail.allowed_actions);
    undertakings.setActiveFromItem(begun.item, {
      leaseId: begun.lease.lease_id,
      leaseGeneration: begun.lease.generation,
      executorKind: "human",
    });
    return {
      lease_id: begun.lease.lease_id,
      generation: begun.lease.generation,
    };
  }

  async function createFile() {
    const entered = newPath.trim().replaceAll("\\", "/");
    const path = selectedDirectory && !entered.includes("/")
      ? `${selectedDirectory}/${entered}`
      : entered;
    if (!path || mutating) return;
    mutating = true;
    error = null;
    try {
      const lease = await ensureLease();
      const source = await createUndertakingSource(workId, { path, ...lease });
      creatingPath = false;
      newPath = "";
      await openUndertakingLocation({ workId, path: source.path, line: 1 });
      await load(false, true);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      mutating = false;
    }
  }

  async function createFolder() {
    const entered = newPath.trim().replaceAll("\\", "/").replace(/\/+$/, "");
    const path = selectedDirectory && !entered.includes("/")
      ? `${selectedDirectory}/${entered}`
      : entered;
    if (!path || mutating) return;
    mutating = true;
    error = null;
    try {
      const lease = await ensureLease();
      await createUndertakingSource(workId, {
        path,
        kind: "directory",
        ...lease,
      });
      creatingFolder = false;
      newPath = "";
      selectedDirectory = path;
      expanded.add(path);
      await load(false, true);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      mutating = false;
    }
  }

  function filesUnderDirectory(directory: string): string[] {
    const prefix = `${directory.replace(/\/+$/, "")}/`;
    return (tree?.files ?? [])
      .map((file) => file.path)
      .filter((path) => path === directory || path.startsWith(prefix));
  }

  async function renameDirectorySelected() {
    const directory = renamingDirectory?.trim() ?? "";
    const destination = renameDestination.trim().replaceAll("\\", "/").replace(/\/+$/, "");
    if (!directory || !destination || destination === directory || mutating) return;
    const paths = filesUnderDirectory(directory);
    if (paths.length === 0) {
      error = "That folder has no files to rename yet.";
      return;
    }
    const dirty = codeWorkspace.tabsFor(workId).some(
      (tab) =>
        codeWorkspace.isDirty(tab) &&
        (tab.path === directory || tab.path.startsWith(`${directory}/`)),
    );
    if (dirty) {
      error = "Save or discard drafts under this folder before renaming it.";
      return;
    }
    mutating = true;
    error = null;
    try {
      const lease = await ensureLease();
      const preconditions = [];
      const operations = [];
      for (const path of paths) {
        const source = await getUndertakingSource(workId, path);
        preconditions.push({
          kind: "existing" as const,
          path,
          expected_digest: source.digest,
        });
        const nextPath = path === directory
          ? destination
          : `${destination}/${path.slice(directory.length + 1)}`;
        preconditions.push({ kind: "missing" as const, path: nextPath });
        operations.push({
          kind: "rename" as const,
          path,
          destination: nextPath,
        });
      }
      await applyUndertakingSourceWorkspaceEdit(workId, {
        preconditions,
        operations,
        ...lease,
      });
      for (const path of paths) {
        const nextPath = path === directory
          ? destination
          : `${destination}/${path.slice(directory.length + 1)}`;
        const open = codeWorkspace.tabsFor(workId).find((tab) => tab.path === path);
        if (open) {
          const source = await getUndertakingSource(workId, nextPath);
          codeWorkspace.replacePath(workId, path, source);
          await lmeWorkspace.replaceCodeFile(workId, path, nextPath);
        }
      }
      renamingDirectory = null;
      renameDestination = "";
      selectedDirectory = destination;
      await load(false, true);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      mutating = false;
    }
  }

  async function deleteDirectorySelected() {
    const directory = selectedDirectory.trim();
    if (!directory || mutating) return;
    const paths = filesUnderDirectory(directory);
    if (paths.length === 0) {
      error = "That folder has no files to delete yet.";
      return;
    }
    const dirty = codeWorkspace.tabsFor(workId).some(
      (tab) =>
        codeWorkspace.isDirty(tab) &&
        (tab.path === directory || tab.path.startsWith(`${directory}/`)),
    );
    if (dirty) {
      error = "Save or discard drafts under this folder before deleting it.";
      return;
    }
    if (!window.confirm(`Delete folder ${directory} and ${paths.length} file${paths.length === 1 ? "" : "s"}?`)) {
      return;
    }
    mutating = true;
    error = null;
    try {
      const lease = await ensureLease();
      const preconditions = [];
      const operations = [];
      for (const path of paths) {
        const source = await getUndertakingSource(workId, path);
        preconditions.push({
          kind: "existing" as const,
          path,
          expected_digest: source.digest,
        });
        operations.push({ kind: "delete" as const, path });
      }
      await applyUndertakingSourceWorkspaceEdit(workId, {
        preconditions,
        operations,
        ...lease,
      });
      for (const path of paths) {
        await lmeWorkspace.closeCodeFile(workId, path);
        codeWorkspace.removePath(workId, path);
      }
      selectedDirectory = "";
      undertakings.setSelection({ path: null, line: null, entityId: null });
      await load(false, true);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      mutating = false;
    }
  }

  async function renameSelected() {
    const path = undertakings.active?.workId === workId
      ? undertakings.active.selectedPath
      : null;
    if (!path || mutating) return;
    const open = codeWorkspace.tabsFor(workId).find((tab) => tab.path === path);
    if (open && codeWorkspace.isDirty(open)) {
      error = "Save or discard this file's draft before renaming it.";
      return;
    }
    const destination = renameDestination.trim();
    if (!destination || destination === path) return;
    mutating = true;
    error = null;
    try {
      const lease = await ensureLease();
      const source = open ?? (await getUndertakingSource(workId, path));
      const renamed = await renameUndertakingSource(workId, {
        path,
        destination,
        expected_digest: source.digest,
        ...lease,
      });
      codeWorkspace.replacePath(workId, path, renamed);
      await lmeWorkspace.replaceCodeFile(workId, path, renamed.path);
      renamingPath = null;
      renameDestination = "";
      undertakings.setSelection({ path: renamed.path, line: 1, entityId: null });
      await load(false, true);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      mutating = false;
    }
  }

  async function deleteSelected() {
    const path = undertakings.active?.workId === workId
      ? undertakings.active.selectedPath
      : null;
    if (!path || mutating) return;
    const open = codeWorkspace.tabsFor(workId).find((tab) => tab.path === path);
    if (open && codeWorkspace.isDirty(open)) {
      error = "Save or discard this file's draft before deleting it.";
      return;
    }
    if (!window.confirm(`Delete ${path} from this project?`)) return;
    mutating = true;
    error = null;
    try {
      const lease = await ensureLease();
      const source = open ?? (await getUndertakingSource(workId, path));
      await deleteUndertakingSource(workId, {
        path,
        expected_digest: source.digest,
        ...lease,
      });
      await lmeWorkspace.closeCodeFile(workId, path);
      codeWorkspace.removePath(workId, path);
      deletedFile = source;
      undertakings.setSelection({ path: null, line: null, entityId: null });
      await load(false, true);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      mutating = false;
    }
  }

  async function undoDelete() {
    const deleted = deletedFile;
    if (!deleted || mutating) return;
    mutating = true;
    error = null;
    try {
      const lease = await ensureLease();
      const created = await createUndertakingSource(workId, { path: deleted.path, ...lease });
      const restored = await saveUndertakingSource(workId, {
        path: deleted.path,
        content: deleted.content,
        expected_digest: created.digest,
        ...lease,
      });
      deletedFile = null;
      await openUndertakingLocation({ workId, path: restored.path, line: 1 });
      await load(false, true);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      mutating = false;
    }
  }

  function toggle(path: string) {
    if (expanded.has(path)) expanded.delete(path);
    else expanded.add(path);
  }

  function statusLabel(status: string): string {
    if (status === "??") return "New";
    if (status.includes("D")) return "Deleted";
    if (status.includes("A")) return "Added";
    if (status.includes("R")) return "Renamed";
    return "Changed";
  }

  $effect(() => {
    void query;
    untrack(() => {
      if (contentSearch) contentSearch = null;
    });
  });

  $effect(() => {
    void workId;
    void prepared;
    void load(false);
    return () => {
      loadToken += 1;
      loading = false;
      building = false;
    };
  });

  $effect(() => {
    const path = undertakings.active?.workId === workId
      ? undertakings.active.selectedPath
      : null;
    if (!path) return;
    const parts = path.split("/");
    for (let index = 1; index < parts.length; index += 1) {
      expanded.add(parts.slice(0, index).join("/"));
    }
  });

  export function searchInFiles() {
    void searchContent();
  }

  export function clearFind() {
    query = "";
    contentSearch = null;
  }
</script>

<div class="flex h-full min-h-0 flex-col border-y border-surface-500/20 bg-surface-950/25 {fill ? 'border-y-0' : ''}">
  {#if !prepared}
    <p class="px-3 py-2 text-[10px] leading-relaxed text-content-secondary">
      Set up this project to see and edit its files.
    </p>
  {:else}
    {#if !chromeInDock}
      <div class="flex items-center gap-1 border-b border-surface-500/20 px-2 py-1">
        <div class="relative min-w-0 flex-1">
          <Search size={11} class="pointer-events-none absolute left-1.5 top-1/2 -translate-y-1/2 text-content-quiet" />
          <input
            bind:this={searchInput}
            type="search"
            class="w-full rounded border border-transparent bg-surface-900/60 py-1 pl-5 pr-1.5 text-[10px] text-surface-200 outline-none focus:border-surface-500/50"
            placeholder="Find a file"
            aria-label="Find a project file"
            bind:value={query}
            oninput={() => {
              if (contentSearch) contentSearch = null;
            }}
            onkeydown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void searchContent();
              }
              if (event.key === "Escape") {
                query = "";
                contentSearch = null;
              }
            }}
          />
        </div>
        <button
          type="button"
          class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-200"
          aria-label="Refresh project files"
          title="Refresh files"
          onclick={() => void load(false, true)}
        ><RefreshCw size={12} class={loading ? "animate-spin" : ""} /></button>
        <button
          type="button"
          class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-200"
          aria-label="New file"
          title="New file"
          onclick={() => dispatchCodeCommand("workbench.action.files.newFile")}
        ><FilePlus2 size={12} /></button>
        <button
          type="button"
          class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-200"
          aria-label="New folder"
          title="New folder"
          onclick={() => dispatchCodeCommand("workbench.action.files.newFolder")}
        ><FolderPlus size={12} /></button>
      </div>
    {/if}

    {#if creatingPath}
      <form
        class="flex gap-1 border-b border-surface-500/20 px-2 py-1"
        onsubmit={(event) => {
          event.preventDefault();
          void createFile();
        }}
      >
        <input
          class="min-w-0 flex-1 rounded border border-surface-500/40 bg-surface-900 px-1.5 py-1 font-mono text-[10px] text-surface-200"
          placeholder={selectedDirectory ? `New file in ${selectedDirectory}` : "src/new_file.rs"}
          aria-label="New source path"
          bind:value={newPath}
        />
        <button type="submit" class="rounded bg-primary-500/80 px-2 text-[9px] text-surface-50" disabled={!newPath.trim() || mutating}>Create</button>
      </form>
    {/if}

    {#if creatingFolder}
      <form
        class="flex gap-1 border-b border-surface-500/20 px-2 py-1"
        onsubmit={(event) => {
          event.preventDefault();
          void createFolder();
        }}
      >
        <input
          class="min-w-0 flex-1 rounded border border-surface-500/40 bg-surface-900 px-1.5 py-1 font-mono text-[10px] text-surface-200"
          placeholder={selectedDirectory ? `New folder in ${selectedDirectory}` : "src/utils"}
          aria-label="New folder path"
          bind:value={newPath}
        />
        <button type="submit" class="rounded bg-primary-500/80 px-2 text-[9px] text-surface-50" disabled={!newPath.trim() || mutating}>Create</button>
      </form>
    {/if}

    {#if renamingPath}
      <form
        class="flex gap-1 border-b border-surface-500/20 px-2 py-1"
        onsubmit={(event) => {
          event.preventDefault();
          void renameSelected();
        }}
      >
        <input
          bind:this={renameInput}
          class="min-w-0 flex-1 rounded border border-surface-500/40 bg-surface-900 px-1 py-0.5 font-mono text-[9px] text-surface-200"
          aria-label="Rename file"
          bind:value={renameDestination}
          onkeydown={(event) => {
            if (event.key === "Escape") renamingPath = null;
          }}
        />
        <button type="submit" class="rounded px-1.5 text-[9px] text-primary-200" disabled={mutating}>Rename</button>
        <button type="button" class="rounded px-1.5 text-[9px] text-content-quiet" onclick={() => (renamingPath = null)}>Cancel</button>
      </form>
    {:else if renamingDirectory}
      <form
        class="flex gap-1 border-b border-surface-500/20 px-2 py-1"
        onsubmit={(event) => {
          event.preventDefault();
          void renameDirectorySelected();
        }}
      >
        <input
          bind:this={renameInput}
          class="min-w-0 flex-1 rounded border border-surface-500/40 bg-surface-900 px-1 py-0.5 font-mono text-[9px] text-surface-200"
          aria-label="Rename folder"
          bind:value={renameDestination}
          onkeydown={(event) => {
            if (event.key === "Escape") renamingDirectory = null;
          }}
        />
        <button type="submit" class="rounded px-1.5 text-[9px] text-primary-200" disabled={mutating}>Rename</button>
        <button type="button" class="rounded px-1.5 text-[9px] text-content-quiet" onclick={() => (renamingDirectory = null)}>Cancel</button>
      </form>
    {/if}

    {#if deletedFile}
      <div class="flex items-center gap-2 border-b border-surface-500/20 bg-surface-900/60 px-2 py-1.5 text-[9px] text-content-secondary">
        <span class="min-w-0 flex-1 truncate">Deleted {deletedFile.path}</span>
        <button type="button" class="flex items-center gap-1 rounded px-1.5 py-0.5 text-primary-200 hover:bg-surface-800" disabled={mutating} onclick={() => void undoDelete()}><Undo2 size={10} />Undo</button>
      </div>
    {/if}

    {#if error}
      <p class="px-3 py-2 text-[10px] text-content-warning">{humanizeForgeMessage(error)}</p>
    {:else if contentSearching}
      <p class="px-3 py-2 text-[10px] text-content-secondary">Searching inside files…</p>
    {:else if contentSearch}
      <div class="min-h-0 flex-1 overflow-y-auto py-1 {fill ? '' : 'max-h-[min(46vh,32rem)]'}" aria-label="File search results">
        {#if contentSearch.hits.length === 0}
          <p class="px-3 py-2 text-[10px] text-content-secondary">No content matches.</p>
        {:else}
          {#each contentSearch.hits as hit (`${hit.path}:${hit.line}`)}
            <button
              type="button"
              class="w-full border-b border-surface-500/15 px-2 py-1.5 text-left hover:bg-surface-800/70"
              onclick={() => void openUndertakingLocation({ workId, path: hit.path, line: hit.line })}
            >
              <span class="block truncate font-mono text-[9px] text-content-link/80">{hit.path}:{hit.line}</span>
              <span class="mt-0.5 block truncate text-[10px] text-content-secondary">{hit.preview}</span>
            </button>
          {/each}
        {/if}
        {#if contentSearch.truncated}
          <p class="px-2 py-1 text-[9px] text-content-warning">First 500 matches shown.</p>
        {/if}
      </div>
    {:else if loading && visibleRows.length === 0}
      <p class="px-3 py-2 text-[10px] text-content-secondary">Loading project files…</p>
      <button
        type="button"
        class="mx-3 mb-2 rounded px-2 py-1 text-[9px] text-content-link hover:bg-surface-800"
        onclick={() => void load(false, true)}
      >Retry</button>
    {:else if building && visibleRows.length === 0}
      <p class="px-3 py-2 text-[10px] text-content-secondary">Building file list…</p>
    {:else if !loading && !building && rows.length === 0}
      <p class="px-3 py-2 text-[10px] text-content-secondary">
        {query ? "No matching files." : "No files yet."}
      </p>
    {:else}
      <div class="code-tree min-h-0 flex-1 overflow-y-auto py-0.5 {fill ? '' : 'max-h-[min(46vh,32rem)]'}" role="tree" aria-label="Project files">
        {#each visibleRows as row (row.path)}
          {@const fileSelected =
            row.kind === "file" &&
            undertakings.active?.workId === workId &&
            undertakings.active.selectedPath === row.path}
          {@const dirSelected = row.kind === "directory" && selectedDirectory === row.path}
          <div
            class="code-tree-row"
            class:code-tree-row--selected={fileSelected || dirSelected}
            class:code-tree-row--dir={row.kind === "directory"}
            class:code-tree-row--nested={row.depth > 0}
            style={`--code-tree-depth: ${row.depth}`}
            role="presentation"
          >
            <button
              type="button"
              role="treeitem"
              aria-expanded={row.kind === "directory" ? expanded.has(row.path) : undefined}
              aria-selected={fileSelected}
              class="code-tree-row-main"
              title={row.path}
              onclick={() => {
                if (row.kind === "directory") {
                  selectedDirectory = row.path;
                  toggle(row.path);
                } else {
                  selectedDirectory = "";
                  void openUndertakingLocation({ workId, path: row.path, line: 1 });
                }
              }}
            >
              {#if row.kind === "directory"}
                {#if expanded.has(row.path)}
                  <ChevronDown size={14} class="code-tree-chevron" />
                {:else}
                  <ChevronRight size={14} class="code-tree-chevron" />
                {/if}
              {:else}
                <span class="code-tree-spacer" aria-hidden="true"></span>
                <CodeFileIcon path={row.path} size={14} />
              {/if}
              <span class="code-tree-name">{query ? row.path : row.name}</span>
              {#if row.kind === "file" && row.status}
                <span
                  class="code-tree-status {row.status === '??' ? 'code-tree-status--new' : 'code-tree-status--changed'}"
                  title={statusLabel(row.status)}
                  aria-label={statusLabel(row.status)}
                ></span>
                <span class="sr-only">{statusLabel(row.status)}</span>
              {/if}
            </button>
            {#if fileSelected || dirSelected}
              <div class="code-tree-actions">
                <button
                  type="button"
                  class="code-tree-action"
                  title={row.kind === "directory" ? "Rename folder" : "Rename file"}
                  aria-label={row.kind === "directory" ? "Rename folder" : "Rename file"}
                  disabled={mutating}
                  onclick={(event) => {
                    event.stopPropagation();
                    if (row.kind === "directory") {
                      renamingPath = null;
                      renamingDirectory = row.path;
                      renameDestination = row.path;
                    } else {
                      renamingDirectory = null;
                      renamingPath = row.path;
                      renameDestination = row.path;
                    }
                    void tick().then(() => renameInput?.focus());
                  }}
                ><Pencil size={11} /></button>
                <button
                  type="button"
                  class="code-tree-action code-tree-action--danger"
                  title={row.kind === "directory" ? "Delete folder" : "Delete file"}
                  aria-label={row.kind === "directory" ? "Delete folder" : "Delete file"}
                  disabled={mutating}
                  onclick={(event) => {
                    event.stopPropagation();
                    if (row.kind === "directory") {
                      selectedDirectory = row.path;
                      void deleteDirectorySelected();
                    } else {
                      void deleteSelected();
                    }
                  }}
                ><Trash2 size={11} /></button>
              </div>
            {/if}
          </div>
        {/each}
        {#if building}
          <p class="px-3 py-1 text-[9px] text-content-secondary">Building remaining files…</p>
        {/if}
        {#if hiddenRowCount > 0}
          <p class="px-3 py-1 text-[9px] text-content-secondary">
            Showing {visibleRows.length} of {rows.length}. Narrow with Find a file.
          </p>
        {/if}
      </div>
      {#if tree?.truncated}
        <p class="border-t border-surface-500/20 px-2 py-1 text-[9px] text-content-warning">
          Showing the first 20,000 files. Search to narrow the view.
        </p>
      {/if}
      {#if query && !contentSearch}
        <p class="border-t border-surface-500/20 px-2 py-1 text-[9px] text-content-secondary">
          Press Enter to search inside files.
        </p>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .code-tree {
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
    text-rendering: optimizeLegibility;
  }

  .code-tree-row {
    /* VS Code explorer ~22px rows, 8px indent steps */
    --code-tree-inset: calc(0.35rem + (var(--code-tree-depth, 0) * 0.75rem));
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.1rem;
    min-height: 22px;
    padding-right: 0.25rem;
    padding-left: var(--code-tree-inset);
  }

  .code-tree-row::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: calc(var(--code-tree-inset) - 0.55rem);
    width: 1px;
    border-radius: 999px;
    background: rgb(var(--color-surface-500) / 0.32);
    opacity: 0;
  }

  .code-tree-row--nested::before {
    opacity: 1;
  }

  .code-tree-row:hover {
    background: rgb(var(--color-surface-800) / 0.5);
  }

  .code-tree-row--selected {
    background: rgb(var(--color-surface-700) / 0.55);
  }

  .code-tree-row--selected:hover {
    background: rgb(var(--color-surface-700) / 0.65);
  }

  .code-tree-row-main {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 0.35rem;
    border: 0;
    background: transparent;
    padding: 0;
    text-align: left;
    /* Near-primary text — secondary was forcing a squint. */
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
    font-weight: 400;
    line-height: 22px;
    letter-spacing: 0;
    cursor: pointer;
  }

  .code-tree-row--selected .code-tree-row-main {
    color: rgb(var(--theme-text));
  }

  .code-tree-row--dir .code-tree-row-main {
    color: rgb(var(--theme-text));
  }

  .code-tree-row--dir .code-tree-name {
    font-weight: 500;
  }

  .code-tree-spacer {
    width: 14px;
    flex-shrink: 0;
  }

  :global(.code-tree-chevron) {
    flex-shrink: 0;
    width: 14px;
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 55%,
      transparent
    );
    opacity: 1;
  }

  .code-tree-name {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .code-tree-status {
    margin-left: auto;
    height: 0.35rem;
    width: 0.35rem;
    flex-shrink: 0;
    border-radius: 999px;
  }

  .code-tree-status--new {
    background: color-mix(
      in srgb,
      rgb(var(--theme-success)) 70%,
      rgb(var(--theme-text-secondary))
    );
  }

  .code-tree-status--changed {
    background: color-mix(
      in srgb,
      rgb(var(--theme-warning)) 70%,
      rgb(var(--theme-text-secondary))
    );
  }

  .code-tree-actions {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.05rem;
  }

  .code-tree-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    padding: 0.2rem;
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
  }

  .code-tree-action:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.55);
    color: rgb(var(--theme-text));
  }

  .code-tree-action--danger:hover:not(:disabled) {
    background: rgb(var(--color-error-500) / 0.15);
    color: rgb(var(--theme-error));
  }

  .code-tree-action:disabled {
    opacity: 0.35;
  }
</style>

<svelte:window
  onkeydown={(event) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "p") {
      event.preventDefault();
      searchInput?.focus();
      searchInput?.select();
    }
  }}
/>
