<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { tick } from "svelte";
  import {
    ChevronDown,
    ChevronRight,
    File,
    FilePlus2,
    Folder,
    FolderOpen,
    RefreshCw,
    Search,
    Pencil,
    Trash2,
    Undo2,
  } from "@lucide/svelte";
  import {
    getUndertakingSource,
    createUndertakingSource,
    renameUndertakingSource,
    deleteUndertakingSource,
    beginHumanAttempt,
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

  interface Props {
    workId: string;
    prepared: boolean;
    /** Fill remaining explorer height instead of a capped max-height. */
    fill?: boolean;
  }

  let { workId, prepared, fill = false }: Props = $props();
  let tree = $state<ForgeSourceTree | null>(null);
  /** Nested nodes built off the critical path — never sync-derived from 20k files. */
  let nodes = $state<CodeSourceTreeNode[]>([]);
  let loading = $state(false);
  let building = $state(false);
  let error = $state<string | null>(null);
  let query = $state("");
  let searchInput = $state<HTMLInputElement | null>(null);
  let contentSearch = $state<ForgeSourceSearch | null>(null);
  let contentSearching = $state(false);
  let creatingPath = $state(false);
  let newPath = $state("");
  let mutating = $state(false);
  let selectedDirectory = $state("");
  let renamingPath = $state<string | null>(null);
  let renameDestination = $state("");
  let deletedFile = $state<ForgeSourceFile | null>(null);
  let renameInput = $state<HTMLInputElement | null>(null);
  const expanded = new SvelteSet<string>();
  /** Monotonic id so only the latest in-flight response may update UI / clear loading. */
  let loadToken = 0;

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
    if (detail?.id !== workId || !detail.allowed_actions.begin_attempt.allowed) {
      throw new Error(
        detail?.allowed_actions.begin_attempt.reason ??
          "This project is not ready for file changes",
      );
    }
    const begun = await beginHumanAttempt(workId);
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
      await codeWorkspace.open(workId, source.path, 1);
      undertakings.setSelection({ path: source.path, line: 1, entityId: null });
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
      await codeWorkspace.open(workId, restored.path, 1);
      undertakings.setSelection({ path: restored.path, line: 1, entityId: null });
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
</script>

<div class="flex h-full min-h-0 flex-col border-y border-surface-500/20 bg-surface-950/25 {fill ? 'border-y-0' : ''}">
  {#if !prepared}
    <p class="px-3 py-2 text-[10px] leading-relaxed text-surface-500">
      Set up this project to see and edit its files.
    </p>
  {:else}
    <div class="flex items-center gap-1 border-b border-surface-500/20 px-2 py-1">
      <div class="relative min-w-0 flex-1">
        <Search size={11} class="pointer-events-none absolute left-1.5 top-1/2 -translate-y-1/2 text-surface-500" />
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
        class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200"
        aria-label="Refresh project files"
        title="Refresh files"
        onclick={() => void load(false, true)}
      ><RefreshCw size={12} class={loading ? "animate-spin" : ""} /></button>
      <button
        type="button"
        class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200"
        aria-label="New file"
        title="New file"
        onclick={() => (creatingPath = !creatingPath)}
      ><FilePlus2 size={12} /></button>
    </div>

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

    {#if undertakings.active?.workId === workId && undertakings.active.selectedPath}
      <div class="flex items-center justify-between gap-2 border-b border-surface-500/20 px-2 py-1">
        {#if renamingPath === undertakings.active.selectedPath}
          <form class="flex min-w-0 flex-1 gap-1" onsubmit={(event) => { event.preventDefault(); void renameSelected(); }}>
            <input bind:this={renameInput} class="min-w-0 flex-1 rounded border border-surface-500/40 bg-surface-900 px-1 py-0.5 font-mono text-[9px] text-surface-200" bind:value={renameDestination} onkeydown={(event) => { if (event.key === "Escape") renamingPath = null; }} />
            <button type="submit" class="rounded px-1.5 text-[9px] text-primary-200">Rename</button>
          </form>
        {:else}
          <span class="min-w-0 flex-1 truncate font-mono text-[9px] text-surface-500">{undertakings.active.selectedPath}</span>
        {/if}
        <button type="button" class="rounded p-1 text-surface-500 hover:bg-surface-800 hover:text-surface-200" title="Rename selected file" aria-label="Rename selected file" disabled={mutating} onclick={() => { renamingPath = undertakings.active?.selectedPath ?? null; renameDestination = renamingPath ?? ""; void tick().then(() => renameInput?.focus()); }}><Pencil size={11} /></button>
        <button type="button" class="rounded p-1 text-surface-500 hover:bg-rose-950/50 hover:text-rose-200" title="Delete selected file" aria-label="Delete selected file" disabled={mutating} onclick={() => void deleteSelected()}><Trash2 size={11} /></button>
      </div>
    {/if}

    {#if deletedFile}
      <div class="flex items-center gap-2 border-b border-surface-500/20 bg-surface-900/60 px-2 py-1.5 text-[9px] text-surface-300">
        <span class="min-w-0 flex-1 truncate">Deleted {deletedFile.path}</span>
        <button type="button" class="flex items-center gap-1 rounded px-1.5 py-0.5 text-primary-200 hover:bg-surface-800" disabled={mutating} onclick={() => void undoDelete()}><Undo2 size={10} />Undo</button>
      </div>
    {/if}

    {#if error}
      <p class="px-3 py-2 text-[10px] text-amber-200">{humanizeForgeMessage(error)}</p>
    {:else if contentSearching}
      <p class="px-3 py-2 text-[10px] text-surface-500">Searching inside files…</p>
    {:else if contentSearch}
      <div class="min-h-0 flex-1 overflow-y-auto py-1 {fill ? '' : 'max-h-[min(46vh,32rem)]'}" aria-label="File search results">
        {#if contentSearch.hits.length === 0}
          <p class="px-3 py-2 text-[10px] text-surface-500">No content matches.</p>
        {:else}
          {#each contentSearch.hits as hit (`${hit.path}:${hit.line}`)}
            <button
              type="button"
              class="w-full border-b border-surface-500/15 px-2 py-1.5 text-left hover:bg-surface-800/70"
              onclick={() => void openUndertakingLocation({ workId, path: hit.path, line: hit.line })}
            >
              <span class="block truncate font-mono text-[9px] text-primary-300/80">{hit.path}:{hit.line}</span>
              <span class="mt-0.5 block truncate text-[10px] text-surface-400">{hit.preview}</span>
            </button>
          {/each}
        {/if}
        {#if contentSearch.truncated}
          <p class="px-2 py-1 text-[9px] text-amber-200">First 500 matches shown.</p>
        {/if}
      </div>
    {:else if loading && visibleRows.length === 0}
      <p class="px-3 py-2 text-[10px] text-surface-500">Loading project files…</p>
      <button
        type="button"
        class="mx-3 mb-2 rounded px-2 py-1 text-[9px] text-primary-300 hover:bg-surface-800"
        onclick={() => void load(false, true)}
      >Retry</button>
    {:else if building && visibleRows.length === 0}
      <p class="px-3 py-2 text-[10px] text-surface-500">Indexing files…</p>
    {:else if !loading && !building && rows.length === 0}
      <p class="px-3 py-2 text-[10px] text-surface-500">
        {query ? "No matching files." : "There are no files to show yet."}
      </p>
    {:else}
      <div class="min-h-0 flex-1 overflow-y-auto py-1 {fill ? '' : 'max-h-[min(46vh,32rem)]'}" role="tree" aria-label="Project files">
        {#each visibleRows as row (row.path)}
          <button
            type="button"
            role="treeitem"
            aria-expanded={row.kind === "directory" ? expanded.has(row.path) : undefined}
            aria-selected={row.kind === "file" && undertakings.active?.workId === workId
              ? undertakings.active.selectedPath === row.path
              : false}
            class="flex w-full items-center gap-1 py-0.5 pr-2 text-left text-[10px] text-surface-300 hover:bg-surface-800/70 hover:text-surface-50"
            style={`padding-left: ${0.35 + row.depth * 0.8}rem`}
            title={row.path}
            onclick={() => {
              if (row.kind === "directory") { selectedDirectory = row.path; toggle(row.path); }
              else void openUndertakingLocation({ workId, path: row.path, line: 1 });
            }}
          >
            {#if row.kind === "directory"}
              {#if expanded.has(row.path)}
                <ChevronDown size={11} class="shrink-0 text-surface-500" />
                <FolderOpen size={12} class="shrink-0 text-primary-300/75" />
              {:else}
                <ChevronRight size={11} class="shrink-0 text-surface-500" />
                <Folder size={12} class="shrink-0 text-primary-300/75" />
              {/if}
            {:else}
              <span class="w-[11px] shrink-0"></span>
              <File size={11} class="shrink-0 text-surface-500" />
            {/if}
            <span class="min-w-0 flex-1 truncate">{query ? row.path : row.name}</span>
            {#if row.kind === "file" && row.status}
              <span
                class="ml-auto h-1.5 w-1.5 shrink-0 rounded-full {row.status === '??' ? 'bg-emerald-400' : 'bg-amber-400'}"
                title={statusLabel(row.status)}
                aria-label={statusLabel(row.status)}
              ></span>
              <span class="sr-only">{statusLabel(row.status)}</span>
            {/if}
          </button>
        {/each}
        {#if building}
          <p class="px-3 py-1 text-[9px] text-surface-500">Indexing remaining files…</p>
        {/if}
        {#if hiddenRowCount > 0}
          <p class="px-3 py-1 text-[9px] text-surface-500">
            Showing {visibleRows.length} of {rows.length}. Narrow with Find a file.
          </p>
        {/if}
      </div>
      {#if tree?.truncated}
        <p class="border-t border-surface-500/20 px-2 py-1 text-[9px] text-amber-200">
          Showing the first 20,000 files. Search to narrow the view.
        </p>
      {/if}
      {#if query && !contentSearch}
        <p class="border-t border-surface-500/20 px-2 py-1 text-[9px] text-surface-500">
          Press Enter to search inside files.
        </p>
      {/if}
    {/if}
  {/if}
</div>

<svelte:window
  onkeydown={(event) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "p") {
      event.preventDefault();
      searchInput?.focus();
      searchInput?.select();
    }
  }}
/>
