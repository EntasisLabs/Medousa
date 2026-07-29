<script lang="ts">
  import { onMount } from "svelte";
  import { Code2, Plus, RefreshCw } from "@lucide/svelte";
  import { humanPhaseLabel, humanizeForgeMessage } from "$lib/forge";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import CodeRepositoryTree from "$lib/components/lme/explorers/CodeRepositoryTree.svelte";

  let creating = $state(false);
  let busy = $state(false);
  let title = $state("");
  let brief = $state("");
  let repoPath = $state("");
  let baseRef = $state("main");
  let error = $state<string | null>(null);

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

  onMount(() => {
    void undertakings.refreshList();
    if (isCoLocatedWorkshop()) repoPath = vault.activeVaultRoot?.path ?? "";
  });

  async function openItem(id: string, label: string) {
    await lmeWorkspace.openCodeWorkspace(id, label);
  }

  async function create() {
    if (!title.trim() || !repoPath.trim() || busy) return;
    busy = true;
    error = null;
    try {
      const item = await undertakings.create({
        title: title.trim(),
        brief: brief.trim() || title.trim(),
        repo_path: repoPath.trim(),
        base_ref: baseRef.trim() || "main",
      });
      creating = false;
      title = "";
      brief = "";
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
      <label class="code-field-label">
        <span>Project</span>
        <input class="code-field" placeholder="What are you changing?" bind:value={title} />
      </label>
      <label class="code-field-label">
        <span>Outcome</span>
        <input class="code-field" placeholder="What should be true when it’s done?" bind:value={brief} />
      </label>
      <label class="code-field-label">
        <span>Repository</span>
        <input
          class="code-field"
          placeholder={isCoLocatedWorkshop() ? "Folder path" : "Folder on connected computer"}
          bind:value={repoPath}
        />
      </label>
      <label class="code-field-label">
        <span>Start from</span>
        <input class="code-field" placeholder="Branch" bind:value={baseRef} />
      </label>
      <button
        type="submit"
        class="rounded bg-primary-500/80 px-2 py-1 text-xs font-medium text-surface-50 disabled:opacity-40"
        disabled={busy || !title.trim() || !repoPath.trim()}
      >Start project</button>
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
