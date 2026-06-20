<script lang="ts">
  import { getGraphemeScript } from "$lib/daemon";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeModuleSummary, GraphemeScriptEntry } from "$lib/types/grapheme";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();

  let search = $state("");
  let subTab = $state<"modules" | "scripts">("modules");
  let selectedModuleId = $state<string | null>(null);
  let selectedScriptId = $state<string | null>(null);

  const mobileDetailOpen = $derived(
    mobile && (selectedModuleId !== null || selectedScriptId !== null),
  );

  $effect(() => {
    if (visible) {
      void workshop.refreshModulesAndScripts();
    }
  });

  const filteredModules = $derived(
    workshop.modules.filter((entry) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return (
        entry.module_id.toLowerCase().includes(needle) ||
        entry.effects.some((effect) => effect.includes(needle))
      );
    }),
  );

  const filteredScripts = $derived(
    workshop.scripts.filter((entry) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return (
        entry.name.toLowerCase().includes(needle) ||
        entry.id.toLowerCase().includes(needle) ||
        entry.modules.some((module) => module.toLowerCase().includes(needle)) ||
        entry.tags.some((tag) => tag.toLowerCase().includes(needle))
      );
    }),
  );

  const selectedModule = $derived(
    selectedModuleId
      ? (workshop.modules.find((entry) => entry.module_id === selectedModuleId) ??
        null)
      : null,
  );

  const selectedScript = $derived(
    selectedScriptId
      ? (workshop.scripts.find((entry) => entry.id === selectedScriptId) ?? null)
      : null,
  );

  function selectModule(entry: GraphemeModuleSummary) {
    selectedModuleId = entry.module_id;
    selectedScriptId = null;
    void workshop.loadModuleDetail(entry.module_id);
  }

  function selectScript(entry: GraphemeScriptEntry) {
    selectedScriptId = entry.id;
    selectedModuleId = null;
    workshop.clearModuleDetail();
  }

  function effectBadgeClass(effect: string): string {
    const normalized = String(effect).toLowerCase();
    if (normalized === "network" || normalized === "secrets") {
      return "text-warning-400/80";
    }
    if (normalized === "pure") {
      return "text-surface-500";
    }
    return "text-surface-400";
  }

  async function runSelectedScript(scriptId: string) {
    const detail = await getGraphemeScript(scriptId);
    await workshop.runScriptSource(detail.body_preview);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  {#if !mobileDetailOpen}
    <div class="flex flex-wrap items-center gap-2 px-4 pt-1">
      <button
        type="button"
        class="rounded-md px-2 py-1 text-[11px] transition {subTab === 'modules'
          ? 'bg-surface-700 text-primary-300 ring-1 ring-inset ring-primary-500/35'
          : 'text-surface-400 hover:bg-surface-800 hover:text-surface-200'}"
        onclick={() => {
          subTab = "modules";
          search = "";
        }}
      >
        Modules · {filteredModules.length}
      </button>
      <button
        type="button"
        class="rounded-md px-2 py-1 text-[11px] transition {subTab === 'scripts'
          ? 'bg-surface-700 text-primary-300 ring-1 ring-inset ring-primary-500/35'
          : 'text-surface-400 hover:bg-surface-800 hover:text-surface-200'}"
        onclick={() => {
          subTab = "scripts";
          search = "";
        }}
      >
        Script library · {filteredScripts.length}
      </button>
    </div>

    <label class="mt-2 block px-4">
      <span class="sr-only">Search {subTab}</span>
      <input
        class="input w-full max-w-md text-sm"
        type="search"
        placeholder={subTab === "modules"
          ? "Search modules…"
          : "Search saved scripts…"}
        bind:value={search}
      />
    </label>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div
      class="workshop-list-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-3 {mobileDetailOpen
        ? 'hidden'
        : ''}"
    >
      {#if workshop.loading && workshop.modules.length === 0}
        <p class="workshop-muted">Loading Grapheme catalog…</p>
      {:else if workshop.error}
        <p class="text-sm text-error-400">{workshop.error}</p>
      {:else if subTab === "modules"}
        {#if filteredModules.length === 0}
          <p class="workshop-muted">No modules match your search.</p>
        {:else}
          <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
            {#each filteredModules as entry (entry.module_id)}
              <li>
                <button
                  type="button"
                  class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedModuleId ===
                  entry.module_id
                    ? 'workshop-list-row-active'
                    : ''}"
                  onclick={() => selectModule(entry)}
                >
                  <div class="min-w-0 flex-1">
                    <div class="flex flex-wrap items-center gap-2">
                      <p class="truncate font-medium text-surface-100">
                        {entry.module_id}
                      </p>
                      <span class="text-[10px] uppercase tracking-wide text-surface-500">
                        {entry.abi}
                      </span>
                    </div>
                    <p class="workshop-faint mt-0.5 font-mono text-[11px]">
                      {entry.op_count} ops · v{entry.version}
                    </p>
                    <div class="mt-1 flex flex-wrap gap-1">
                      {#each entry.effects as effect (effect)}
                        <span
                          class="text-[10px] uppercase tracking-wide {effectBadgeClass(effect)}"
                        >
                          {effect}
                        </span>
                      {/each}
                    </div>
                  </div>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if filteredScripts.length === 0}
        <p class="workshop-muted">
          {search.trim()
            ? "No scripts match your search."
            : "No saved scripts yet — agents can save Grapheme scripts during turns."}
        </p>
      {:else}
        <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
          {#each filteredScripts as entry (entry.id)}
            <li>
              <button
                type="button"
                class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedScriptId ===
                entry.id
                  ? 'workshop-list-row-active'
                  : ''}"
                onclick={() => selectScript(entry)}
              >
                <div class="min-w-0 flex-1">
                  <p class="truncate font-medium text-surface-100">{entry.name}</p>
                  <p class="workshop-faint mt-0.5 font-mono text-[11px]">
                    {entry.id} · v{entry.version}
                  </p>
                  {#if entry.modules.length > 0}
                    <p class="workshop-faint mt-0.5 truncate text-[11px]">
                      {entry.modules.join(", ")}
                    </p>
                  {/if}
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <aside
      class="{mobile
        ? mobileDetailOpen
          ? 'mobile-you-scroll flex min-h-0 flex-1 flex-col overflow-y-auto'
          : 'hidden'
        : 'workshop-detail-pane w-[min(360px,40%)] shrink-0 overflow-y-auto border-l border-surface-500/40'} px-4 py-4"
    >
      {#if mobileDetailOpen}
        <button
          type="button"
          class="workshop-text-action mb-3 shrink-0 text-sm"
          onclick={() => {
            selectedModuleId = null;
            selectedScriptId = null;
            workshop.clearModuleDetail();
          }}
        >
          ← Back to list
        </button>
      {/if}

      {#if selectedModule && workshop.moduleDetailLoading}
        <p class="workshop-muted text-sm">Loading module detail…</p>
      {:else if selectedModule && workshop.moduleDetailError}
        <p class="text-sm text-warning-400">{workshop.moduleDetailError}</p>
      {:else if selectedModule && workshop.moduleDetail}
        <h2 class="workshop-section-title">Module detail</h2>
        <p class="mt-2 font-medium text-surface-100">
          {workshop.moduleDetail.info.module_id}
        </p>
        <p class="workshop-faint mt-1 font-mono text-[11px]">
          {workshop.moduleDetail.info.entrypoint} · {workshop.moduleDetail.info.abi}
        </p>

        <dl class="mt-4 space-y-2 text-xs">
          <div>
            <dt class="workshop-label">Ops</dt>
            <dd class="mt-0.5 text-surface-200">
              {workshop.moduleDetail.info.exported_ops.length}
            </dd>
          </div>
          {#if workshop.moduleDetail.examples.length > 0}
            <div>
              <dt class="workshop-label">Examples</dt>
              <dd class="mt-0.5 space-y-1 font-mono text-[11px] text-surface-300">
                {#each workshop.moduleDetail.examples as example (example)}
                  <p>{example}</p>
                {/each}
              </dd>
            </div>
          {/if}
        </dl>

        <div class="mt-4">
          <h3 class="workshop-label">Exported ops</h3>
          <ul class="mt-2 max-h-64 space-y-2 overflow-y-auto">
            {#each workshop.moduleDetail.info.exported_ops as op (op.op)}
              <li class="rounded-md border border-surface-500/35 px-3 py-2 text-xs">
                <div class="flex flex-wrap items-center gap-2">
                  <span class="font-mono text-surface-100">{op.op}</span>
                  <span
                    class="text-[10px] uppercase tracking-wide {effectBadgeClass(op.effect)}"
                  >
                    {op.effect}
                  </span>
                  <span class="text-[10px] uppercase tracking-wide text-surface-500">
                    {op.stability}
                  </span>
                </div>
                <p class="workshop-faint mt-1">{op.output_type}</p>
              </li>
            {/each}
          </ul>
        </div>
      {:else if selectedScript}
        <h2 class="workshop-section-title">Script detail</h2>
        <p class="mt-2 font-medium text-surface-100">{selectedScript.name}</p>
        <p class="workshop-faint mt-1 font-mono text-[11px]">{selectedScript.id}</p>
        {#if selectedScript.intent}
          <p class="mt-3 text-sm leading-relaxed text-surface-300">
            {selectedScript.intent}
          </p>
        {/if}
        <dl class="mt-4 space-y-2 text-xs">
          {#if selectedScript.modules.length > 0}
            <div>
              <dt class="workshop-label">Modules</dt>
              <dd class="mt-0.5 text-surface-200">
                {selectedScript.modules.join(", ")}
              </dd>
            </div>
          {/if}
          {#if selectedScript.tags.length > 0}
            <div>
              <dt class="workshop-label">Tags</dt>
              <dd class="mt-0.5 text-surface-200">{selectedScript.tags.join(", ")}</dd>
            </div>
          {/if}
        </dl>
        <button
          type="button"
          class="workshop-text-action mt-5"
          disabled={workshop.runBusy}
          onclick={() => void runSelectedScript(selectedScript.id)}
        >
          {workshop.runBusy ? "Running…" : "Run in sandbox"}
        </button>
        {#if workshop.runError}
          <p class="mt-3 text-xs text-error-400">{workshop.runError}</p>
        {:else if workshop.runResult}
          <p class="mt-3 text-xs text-surface-300">
            {workshop.runResult.result.succeeded ? "Succeeded" : "Failed"} · job
            {workshop.runResult.result.job_id ?? "—"}
          </p>
        {/if}
      {:else}
        <p class="workshop-muted text-sm">
          Select a module to inspect ops and examples, or a saved script to run in
          the Grapheme sandbox.
        </p>
      {/if}
    </aside>
  </div>
</div>
