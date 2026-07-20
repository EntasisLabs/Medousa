<script lang="ts">
  import { Blocks, Check, ChevronDown, Search, Shield } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { prepareModuleInsert, qualifyModuleOp } from "$lib/grapheme/graphemeModuleSnippet";
  import {
    listCommonModules,
    loadLastModuleId,
    moduleBlurb,
    moduleJobLabel,
    opHumanTitle,
    pickDefaultModuleId,
    saveLastModuleId,
  } from "$lib/grapheme/scriptWorkbenchHelpers";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeModuleSummary } from "$lib/types/grapheme";

  let open = $state(false);
  let showAllModules = $state(false);
  let allowlistOpen = $state(false);
  let opSearch = $state("");
  let selectedModuleId = $state<string | null>(null);
  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let panelStyle = $state("");

  function placePanel() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const width = Math.min(28 * 16, window.innerWidth - 24);
    let left = rect.right - width;
    left = Math.max(12, Math.min(left, window.innerWidth - width - 12));
    const top = Math.min(rect.bottom + 6, window.innerHeight - 48);
    panelStyle = `top:${top}px;left:${left}px;width:${width}px;`;
  }

  const commonModules = $derived(listCommonModules(workshop.modules, 6));
  const otherModules = $derived.by(() => {
    const commonIds = new Set(commonModules.map((entry) => entry.module_id));
    return workshop.modules.filter((entry) => !commonIds.has(entry.module_id));
  });
  const visibleModules = $derived(
    showAllModules ? [...commonModules, ...otherModules] : commonModules,
  );
  const selectedModule = $derived(
    selectedModuleId
      ? (workshop.modules.find((entry) => entry.module_id === selectedModuleId) ?? null)
      : null,
  );
  const moduleDetail = $derived(
    selectedModuleId && workshop.moduleDetail?.info.module_id === selectedModuleId
      ? workshop.moduleDetail
      : null,
  );
  const filteredOps = $derived.by(() => {
    const ops = moduleDetail?.info.exported_ops ?? [];
    const needle = opSearch.trim().toLowerCase();
    if (!needle) return ops;
    return ops.filter(
      (op) =>
        op.op.toLowerCase().includes(needle) ||
        opHumanTitle(op.op).toLowerCase().includes(needle) ||
        op.effect.toLowerCase().includes(needle) ||
        op.output_type.toLowerCase().includes(needle),
    );
  });
  const triggerLabel = $derived(
    selectedModule ? moduleJobLabel(selectedModule.module_id) : "Modules",
  );

  onMount(() => {
    const onDocClick = (event: MouseEvent) => {
      if (!open) return;
      const target = event.target as Node | null;
      if (menuEl?.contains(target) || triggerEl?.contains(target)) return;
      open = false;
      allowlistOpen = false;
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        open = false;
        allowlistOpen = false;
      }
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  });

  $effect(() => {
    if (!open) return;
    if (workshop.modules.length === 0) {
      void workshop.refreshModulesAndScripts();
    }
  });

  $effect(() => {
    if (!open) return;
    if (workshop.modules.length === 0) return;
    if (
      selectedModuleId &&
      workshop.modules.some((entry) => entry.module_id === selectedModuleId)
    ) {
      return;
    }
    const next = pickDefaultModuleId(workshop.modules) ?? workshop.modules[0]!.module_id;
    selectedModuleId = next;
  });

  $effect(() => {
    if (!open || !selectedModuleId) return;
    void workshop.loadModuleDetail(selectedModuleId);
  });

  async function toggleOpen() {
    open = !open;
    if (open) {
      allowlistOpen = false;
      opSearch = "";
      showAllModules = false;
      placePanel();
      if (workshop.modules.length === 0) {
        await workshop.refreshModulesAndScripts();
      }
      const preferred = loadLastModuleId();
      if (preferred && workshop.modules.some((entry) => entry.module_id === preferred)) {
        selectedModuleId = preferred;
      }
    }
  }

  $effect(() => {
    if (!open) return;
    placePanel();
    const onResize = () => placePanel();
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onResize, true);
    };
  });

  function selectModule(entry: GraphemeModuleSummary) {
    selectedModuleId = entry.module_id;
    saveLastModuleId(entry.module_id);
    opSearch = "";
  }

  function insertOp(op: string) {
    if (!selectedModuleId) return;
    graphemeScriptEditor.ensureInitialTab();
    const examples = moduleDetail?.examples ?? [];
    const body = graphemeScriptEditor.activeTab?.body ?? "";
    const qualified = qualifyModuleOp(selectedModuleId, op);
    graphemeScriptEditor.queueInsert(prepareModuleInsert(body, qualified, examples));
    lmeWorkspace.syncScriptTabFromEditor({ activate: true });
    saveLastModuleId(selectedModuleId);
  }
</script>

<div class="grapheme-module-library">
  <button
    bind:this={triggerEl}
    type="button"
    class="scripts-workbench-toolbar-btn {open ? 'scripts-workbench-toolbar-btn-active' : ''}"
    title="Modules"
    aria-label="Open module library"
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={() => void toggleOpen()}
  >
    <Blocks size={15} strokeWidth={1.75} />
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      class="grapheme-module-library-panel"
      style={panelStyle}
      role="dialog"
      aria-label="Modules"
    >
      <div class="grapheme-module-library-head">
        <div class="min-w-0">
          <p class="text-[11px] font-semibold tracking-[-0.01em] text-surface-100">Modules</p>
          <p class="mt-0.5 truncate text-[10px] text-surface-500">{triggerLabel}</p>
        </div>
        <button
          type="button"
          class="workshop-text-action inline-flex items-center gap-0.5 text-[10px] text-surface-500"
          onclick={() => (showAllModules = !showAllModules)}
        >
          {showAllModules ? "Common" : "View more"}
          <ChevronDown
            size={12}
            strokeWidth={2}
            class="transition {showAllModules ? 'rotate-180' : ''}"
          />
        </button>
      </div>

      <div class="grapheme-module-library-body">
        <div class="grapheme-module-library-modules" role="listbox" aria-label="Modules">
          {#if workshop.loading && workshop.modules.length === 0}
            <p class="px-2 py-3 text-[11px] text-surface-500">Loading…</p>
          {:else if visibleModules.length === 0}
            <p class="px-2 py-3 text-[11px] text-surface-500">No modules.</p>
          {:else}
            {#each visibleModules as entry (entry.module_id)}
              <button
                type="button"
                role="option"
                aria-selected={selectedModuleId === entry.module_id}
                class="grapheme-module-library-module {selectedModuleId === entry.module_id
                  ? 'grapheme-module-library-module-active'
                  : ''}"
                onclick={() => selectModule(entry)}
              >
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-[11px] font-medium text-surface-100">
                    {moduleJobLabel(entry.module_id)}
                  </span>
                  <span class="mt-0.5 block truncate font-mono text-[9px] text-surface-600">
                    {entry.module_id}
                  </span>
                </span>
                {#if selectedModuleId === entry.module_id}
                  <Check size={12} strokeWidth={2.5} class="shrink-0 text-primary-300" />
                {:else}
                  <span class="shrink-0 text-[9px] tabular-nums text-surface-600">
                    {entry.op_count}
                  </span>
                {/if}
              </button>
            {/each}
            {#if !showAllModules && otherModules.length > 0}
              <button
                type="button"
                class="grapheme-module-library-more"
                onclick={() => (showAllModules = true)}
              >
                +{otherModules.length} more
              </button>
            {/if}
          {/if}
        </div>

        <div class="grapheme-module-library-ops">
          <div class="grapheme-module-library-search">
            <Search size={13} class="shrink-0 text-surface-500" />
            <input
              class="grapheme-module-library-search-input"
              type="search"
              placeholder="Search in {selectedModule
                ? moduleJobLabel(selectedModule.module_id)
                : 'module'}…"
              bind:value={opSearch}
            />
          </div>

          {#if selectedModule}
            <p class="grapheme-module-library-blurb">{moduleBlurb(selectedModule)}</p>
          {/if}

          <ul class="grapheme-module-library-op-list" role="listbox" aria-label="Actions">
            {#if workshop.moduleDetailLoading && !moduleDetail}
              <li class="px-2 py-3 text-[11px] text-surface-500">Loading…</li>
            {:else if workshop.moduleDetailError}
              <li class="px-2 py-3 text-[11px] text-warning-400">{workshop.moduleDetailError}</li>
            {:else if filteredOps.length === 0}
              <li class="px-2 py-3 text-[11px] text-surface-500">
                {opSearch.trim() ? "No matches." : "No actions."}
              </li>
            {:else}
              {#each filteredOps as op (op.op)}
                <li>
                  <button
                    type="button"
                    class="grapheme-module-library-op"
                    role="option"
                    onclick={() => insertOp(op.op)}
                  >
                    <span class="min-w-0 flex-1">
                      <span class="block truncate text-[12px] font-medium tracking-[-0.01em] text-surface-50">
                        {opHumanTitle(op.op)}
                      </span>
                      <span class="mt-0.5 block truncate font-mono text-[10px] text-surface-600">
                        {op.op}() → {op.output_type}
                      </span>
                    </span>
                    <span class="grapheme-module-library-insert">Insert</span>
                  </button>
                </li>
              {/each}
            {/if}
          </ul>
        </div>
      </div>

      <div class="grapheme-module-library-footer">
        <button
          type="button"
          class="grapheme-module-library-footer-btn"
          aria-expanded={allowlistOpen}
          onclick={() => (allowlistOpen = !allowlistOpen)}
        >
          <Shield size={12} strokeWidth={2} />
          Allowlist
        </button>
      </div>

      {#if allowlistOpen}
        <div class="grapheme-module-library-allowlist">
          <p class="px-3 pb-1.5 text-[10px] text-surface-500">
            Restrict which modules scripts may use at runtime.
          </p>
          {#if workshop.allowlistError}
            <p class="px-3 pb-1.5 text-[10px] text-error-400">{workshop.allowlistError}</p>
          {/if}
          <ul class="max-h-36 space-y-1 overflow-y-auto px-3 pb-2">
            {#each workshop.modules as entry (entry.module_id)}
              <li class="flex items-center gap-2 text-[11px]">
                <input
                  id="gml-allow-{entry.module_id}"
                  type="checkbox"
                  checked={workshop.isModuleAllowed(entry.module_id)}
                  disabled={workshop.allowlistBusy}
                  onchange={(event) =>
                    workshop.toggleAllowlistModule(
                      entry.module_id,
                      (event.currentTarget as HTMLInputElement).checked,
                    )}
                />
                <label for="gml-allow-{entry.module_id}" class="min-w-0 truncate">
                  <span class="text-surface-200">{moduleJobLabel(entry.module_id)}</span>
                  <span class="ml-1 font-mono text-[10px] text-surface-600">{entry.module_id}</span>
                </label>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </div>
  {/if}
</div>
