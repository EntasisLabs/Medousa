<script lang="ts">
  import { untrack } from "svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import { humanizeForgeMessage } from "$lib/code/undertakingCommandController";
  import {
    findWorldEntities,
    loadWorldImpact,
    loadWorldOverview,
    rebuildWorldMap,
    revealWorldLocation,
    selectedWorldSnapshot,
    worldSlotState,
    type WorldAvecResult,
    type WorldBindingStatus,
    type WorldFilesResult,
    type WorldFindResult,
    type WorldImpactResult,
    type WorldLocationIntent,
    type WorldSnapshotKind,
  } from "$lib/work/undertakingWorldController";

  interface Props {
    workId: string;
    locate?: WorldLocationIntent | null;
    onClose: () => void;
    onError?: (message: string | null) => void;
  }

  let { workId, locate = null, onClose, onError }: Props = $props();

  let worldInsight = $state<WorldAvecResult | null>(null);
  let worldFiles = $state<WorldFilesResult | null>(null);
  let worldFind = $state<WorldFindResult | null>(null);
  let worldImpact = $state<WorldImpactResult | null>(null);
  let worldError = $state<string | null>(null);
  let worldBinding = $state<WorldBindingStatus | null>(null);
  let findQuery = $state("");
  let impactEntity = $state("");
  let busy = $state(false);
  let worldSnapshot = $state<WorldSnapshotKind>("sealed");

  const worldSlotStateValue = $derived(worldSlotState(worldBinding, worldSnapshot));
  const worldMapIndexing = $derived(
    worldSlotStateValue === "queued" || worldSlotStateValue === "indexing",
  );
  const worldMapFailed = $derived(worldSlotStateValue === "failed");
  const worldMapReady = $derived(
    worldSlotStateValue === "ready" && worldInsight != null && !worldError,
  );

  function applyOverview(result: Awaited<ReturnType<typeof loadWorldOverview>>) {
    worldBinding = result.binding;
    worldFiles = result.files;
    worldInsight = result.insight;
    worldError = result.error;
    if (result.resetSearch) {
      worldFind = null;
      worldImpact = null;
    }
  }

  async function refreshOverview() {
    busy = true;
    worldError = null;
    try {
      applyOverview(await loadWorldOverview(workId, worldSnapshot));
    } finally {
      busy = false;
    }
  }

  async function rebuild() {
    onError?.(null);
    busy = true;
    try {
      applyOverview(await rebuildWorldMap(workId, worldSnapshot));
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
      await refreshOverview();
    } finally {
      busy = false;
    }
  }

  async function runWorld(fn: () => Promise<void>) {
    busy = true;
    onError?.(null);
    try {
      await fn();
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function reveal(input: WorldLocationIntent) {
    const snapshot = selectedWorldSnapshot(worldBinding, worldSnapshot);
    const located = await revealWorldLocation(workId, input, snapshot);
    impactEntity = located?.entityId ?? input.entityId ?? "";
    if (located?.impact) worldImpact = located.impact;
  }

  $effect(() => {
    const id = workId;
    const kind = worldSnapshot;
    if (!id) return;
    void (async () => {
      busy = true;
      worldError = null;
      try {
        applyOverview(await loadWorldOverview(id, kind));
      } finally {
        busy = false;
      }
    })();
  });

  $effect(() => {
    const intent = locate;
    const id = workId;
    if (!intent || !id) return;
    void untrack(() => reveal(intent));
  });
</script>

<div class="world-panel">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div>
      <h4 class="text-sm font-semibold">Understand this code</h4>
      <p class="workshop-faint mt-0.5 text-[10px]">
        See relationships and possible impact without leaving your work
      </p>
    </div>
    <div class="flex items-center gap-1 text-[10px]">
      <button
        type="button"
        class="rounded px-2 py-0.5 {worldSnapshot === 'baseline'
          ? 'bg-surface-700 text-surface-50'
          : 'text-content-tertiary'}"
        onclick={() => (worldSnapshot = "baseline")}
      >
        Before
      </button>
      <button
        type="button"
        class="rounded px-2 py-0.5 {worldSnapshot === 'sealed'
          ? 'bg-surface-700 text-surface-50'
          : 'text-content-tertiary'}"
        onclick={() => (worldSnapshot = "sealed")}
      >
        Current
      </button>
      <button
        type="button"
        class="ml-1 rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-surface-200"
        aria-label="Close code understanding"
        title="Close"
        onclick={onClose}
      >×</button>
    </div>
  </div>
  <p class="mt-1 text-[10px] text-content-quiet">
    This view only explains the code; it never changes files.
  </p>

  {#if worldMapReady}
    <div class="mt-2 flex flex-wrap gap-1">
      <button
        type="button"
        class="rounded border border-surface-500/50 px-2 py-1 text-xs"
        onclick={() => void refreshOverview()}
      >
        Refresh understanding
      </button>
      <button
        type="button"
        class="rounded border border-surface-500/50 px-2 py-1 text-xs"
        onclick={() => void rebuild()}
      >
        Rebuild code map
      </button>
    </div>
    {#if worldBinding}
      <details class="mt-2 text-[10px] text-content-quiet">
        <summary class="w-fit cursor-pointer hover:text-content-secondary">Technical details</summary>
        <p class="mt-1">
          Before: {worldBinding.baseline?.state ?? "not indexed"} · current:
          {worldBinding.sealed?.state ?? "not indexed"}
        </p>
        {#if worldBinding.capabilities}
          <div class="mt-1 flex flex-wrap gap-1">
          {#each Object.entries(worldBinding.capabilities).filter(([key]) => key !== "note") as [capability, enabled]}
            <span
              class="rounded-full border border-surface-500/30 px-1.5 py-0.5 text-[9px] {enabled
                ? 'text-content-secondary'
                : 'text-content-faint'}"
            >{capability.replaceAll("_", " ")}{enabled ? "" : " · unavailable"}</span>
          {/each}
          </div>
        {/if}
        {#if worldBinding.diagnostics?.length}
          <ul class="mt-1 text-[10px] text-amber-200/90">
          {#each worldBinding.diagnostics as d}
            <li>{d}</li>
          {/each}
          </ul>
        {/if}
      </details>
    {/if}
    <div class="mt-2 flex flex-wrap items-center gap-1">
      <input
        class="min-w-[120px] flex-1 rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
        placeholder="Find a class, function, or name…"
        bind:value={findQuery}
      />
      <button
        type="button"
        class="rounded border border-surface-500/50 px-2 py-1 text-xs"
        onclick={() =>
          void runWorld(async () => {
            worldFind = await findWorldEntities(
              workId,
              findQuery,
              selectedWorldSnapshot(worldBinding, worldSnapshot),
            );
          })}
      >
        Find
      </button>
    </div>
    <div class="mt-1 flex flex-wrap items-center gap-1">
      <input
        class="min-w-[120px] flex-1 rounded border border-surface-500/40 bg-surface-900 px-2 py-1 text-xs"
        placeholder="Class or function to check"
        bind:value={impactEntity}
      />
      <button
        type="button"
        class="rounded border border-surface-500/50 px-2 py-1 text-xs"
        disabled={!impactEntity.trim()}
        onclick={() =>
          void runWorld(async () => {
            worldImpact = await loadWorldImpact(
              workId,
              impactEntity,
              selectedWorldSnapshot(worldBinding, worldSnapshot),
            );
          })}
      >
        See impact
      </button>
    </div>
    {#if worldFind}
      <div class="mt-2 max-h-44 overflow-auto rounded-md border border-surface-500/25">
        {#if worldFind.entities.length === 0}
          <p class="p-2 text-[10px] text-content-quiet">Nothing matched that name.</p>
        {:else}
          {#each worldFind.entities as entity (entity.id)}
            <button
              type="button"
              class="flex w-full items-center justify-between gap-2 border-b border-surface-500/20 px-2 py-1.5 text-left last:border-0 hover:bg-surface-800/60"
              onclick={() => {
                void reveal({
                  path: entity.path,
                  line: entity.line_start,
                  entityId: entity.id,
                });
              }}
            >
              <span class="min-w-0">
                <span class="block truncate text-[11px] text-surface-200">{entity.label}</span>
                <span class="block truncate font-mono text-[9px] text-content-quiet">{entity.path}</span>
              </span>
              <span class="shrink-0 text-[9px] text-content-quiet">{entity.kind}</span>
            </button>
          {/each}
        {/if}
      </div>
    {/if}
    {#if worldImpact}
      <div class="mt-2 rounded-md border border-surface-500/25 p-2">
        <p class="text-[11px] font-medium text-surface-200">
          What depends on this · {worldImpact.direct_dependents ?? 0} directly,
          {worldImpact.transitive_dependents ?? 0} through other code
        </p>
        {#if worldImpact.message}
          <p class="mt-1 text-[10px] text-content-quiet">{worldImpact.message}</p>
        {/if}
        <ul class="mt-1 max-h-32 overflow-auto text-[10px] text-content-tertiary">
          {#each worldImpact.nodes as node (node.id)}
            <li class="truncate py-0.5">{node.label} <span class="text-content-faint">· {node.path}</span></li>
          {/each}
        </ul>
      </div>
    {/if}
    {#if worldInsight}
      <div class="mt-2 grid gap-2 sm:grid-cols-3">
        <div class="rounded-md bg-surface-900/60 p-2">
          <p class="text-lg font-semibold text-surface-100">
            {worldInsight.code_avec?.fully_scored_entities ?? 0}
          </p>
          <p class="text-[9px] text-content-quiet">fully understood</p>
        </div>
        <div class="rounded-md bg-surface-900/60 p-2">
          <p class="text-lg font-semibold text-surface-100">
            {worldInsight.code_avec?.scoreable_entities ?? 0}
          </p>
          <p class="text-[9px] text-content-quiet">code elements found</p>
        </div>
        <div class="rounded-md bg-surface-900/60 p-2">
          <p class="text-lg font-semibold text-surface-100">
            {worldInsight.code_avec?.gaps.length ?? 0}
          </p>
          <p class="text-[9px] text-content-quiet">still unclear</p>
        </div>
      </div>
    {/if}
    {#if worldFiles}
      <details class="mt-2">
        <summary class="cursor-pointer text-[10px] text-content-tertiary">
          Files in this view · {worldFiles.files.length}
        </summary>
        <ul class="mt-1 max-h-48 overflow-auto rounded-md border border-surface-500/25">
          {#each worldFiles.files as file (file.id)}
            <li class="border-b border-surface-500/15 px-2 py-1 last:border-0">
              <button
                type="button"
                class="w-full truncate text-left font-mono text-[10px] text-content-tertiary hover:text-surface-100"
                onclick={() =>
                  void reveal({ path: file.path, line: 1, entityId: file.id })}
              >{file.path}</button>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  {:else}
    <div class="mt-6 flex flex-1 flex-col items-center justify-center px-2">
      {#if busy || worldMapIndexing}
        <p class="workshop-faint text-sm">Building the code map…</p>
        <p class="mt-1 max-w-xs text-center text-[10px] leading-relaxed text-content-quiet">
          Relationships and impact stay hidden until indexing finishes.
        </p>
      {:else}
        <EmptyState
          title={worldMapFailed ? "Code map failed" : "Code map isn’t ready"}
          description={worldError
            ? humanizeForgeMessage(worldError)
            : worldMapFailed
              ? "Indexing didn’t finish. Rebuild the code map and try again."
              : "Build a map of this project to find symbols and see what depends on them."}
        >
          <div class="flex flex-wrap items-center justify-center gap-2">
            <button
              type="button"
              class="rounded bg-primary-500/80 px-3 py-1.5 text-[11px] font-medium text-surface-50"
              disabled={busy}
              onclick={() => void rebuild()}
            >Rebuild code map</button>
            <button
              type="button"
              class="rounded border border-surface-500/40 px-3 py-1.5 text-[11px] text-surface-200 hover:bg-surface-800"
              disabled={busy}
              onclick={() => void refreshOverview()}
            >Refresh</button>
          </div>
        </EmptyState>
      {/if}
    </div>
    {#if worldBinding}
      <details class="mt-auto pt-4 text-[10px] text-content-quiet">
        <summary class="w-fit cursor-pointer hover:text-content-secondary">Technical details</summary>
        <p class="mt-1">
          Before: {worldBinding.baseline?.state ?? "not indexed"} · current:
          {worldBinding.sealed?.state ?? "not indexed"}
        </p>
      </details>
    {/if}
  {/if}
</div>

<style>
  .world-panel {
    position: absolute;
    inset-block: 0;
    right: 0;
    z-index: 30;
    display: flex;
    width: min(32rem, 100%);
    flex-direction: column;
    overflow: auto;
    border-left: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-950) / 0.98);
    padding: 0.75rem;
    box-shadow: 0 25px 50px -12px rgb(0 0 0 / 0.25);
  }
</style>
