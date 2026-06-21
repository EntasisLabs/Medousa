<script lang="ts">
  import {
    Blocks,
    ChevronLeft,
    ChevronRight,
    FileCode2,
    LayoutTemplate,
    MessageSquare,
    Plus,
  } from "@lucide/svelte";
  import ScriptWorkbenchChatPanel from "$lib/components/automations/ScriptWorkbenchChatPanel.svelte";
  import {
    applyRecipeToEditor,
    GRAPHEME_STARTER_RECIPES,
    type GraphemeRecipe,
  } from "$lib/grapheme/graphemeRecipes";
  import { prepareModuleInsert } from "$lib/grapheme/graphemeModuleSnippet";
  import {
    effectBadgeClass,
    moduleBlurb,
    stabilityLabel,
  } from "$lib/grapheme/scriptWorkbenchHelpers";
  import { haptic } from "$lib/haptics";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import type { GraphemeModuleSummary, GraphemeScriptEntry } from "$lib/types/grapheme";

  interface Props {
    open: boolean;
    visible: boolean;
    initialView?: ToolsView;
    onOpen: () => void;
    onClose: () => void;
    onInserted?: () => void;
  }

  type ToolsView =
    | "root"
    | "templates"
    | "library"
    | "modules-list"
    | "modules-detail"
    | "chat";

  let {
    open,
    visible,
    initialView = "root",
    onOpen,
    onClose,
    onInserted,
  }: Props = $props();

  let view = $state<ToolsView>("root");
  let search = $state("");
  let selectedModuleId = $state<string | null>(null);

  $effect(() => {
    if (!open) {
      view = "root";
      search = "";
      selectedModuleId = null;
      return;
    }
    view = initialView;
    search = "";
  });

  const sheetTitle = $derived(
    view === "root"
      ? "Script tools"
      : view === "templates"
        ? "Templates"
        : view === "library"
          ? "Library"
          : view === "modules-list"
            ? "Modules"
            : view === "modules-detail"
              ? (selectedModuleId ?? "Module")
              : view === "chat"
                ? "Script chat"
                : "Script tools",
  );

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
        entry.id.toLowerCase().includes(needle)
      );
    }),
  );

  const filteredRecipes = $derived(
    GRAPHEME_STARTER_RECIPES.filter((recipe) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return (
        recipe.title.toLowerCase().includes(needle) ||
        recipe.subtitle.toLowerCase().includes(needle)
      );
    }),
  );

  const selectedModule = $derived(
    selectedModuleId
      ? (workshop.modules.find((entry) => entry.module_id === selectedModuleId) ?? null)
      : null,
  );

  const moduleDetailForSelected = $derived(
    selectedModuleId && workshop.moduleDetail?.info.module_id === selectedModuleId
      ? workshop.moduleDetail
      : null,
  );

  const filteredModuleOps = $derived.by(() => {
    const ops = moduleDetailForSelected?.info.exported_ops ?? [];
    const needle = search.trim().toLowerCase();
    if (!needle) return ops;
    return ops.filter(
      (op) =>
        op.op.toLowerCase().includes(needle) ||
        op.effect.toLowerCase().includes(needle) ||
        op.output_type.toLowerCase().includes(needle),
    );
  });

  function closeAll() {
    haptic("light");
    onClose();
  }

  function goTo(next: ToolsView) {
    haptic("light");
    search = "";
    view = next;
  }

  function goBack() {
    haptic("light");
    if (view === "modules-detail") {
      view = "modules-list";
      search = "";
      return;
    }
    view = "root";
    search = "";
  }

  function selectModule(entry: GraphemeModuleSummary) {
    haptic("light");
    selectedModuleId = entry.module_id;
    view = "modules-detail";
    search = "";
    void workshop.loadModuleDetail(entry.module_id);
  }

  function insertOp(op: string) {
    graphemeScriptEditor.ensureInitialTab();
    const examples = workshop.moduleDetail?.examples ?? [];
    const body = graphemeScriptEditor.activeTab?.body ?? "";
    graphemeScriptEditor.queueInsert(prepareModuleInsert(body, op, examples));
    haptic("success");
    onInserted?.();
    closeAll();
  }

  function applyTemplate(recipe: GraphemeRecipe) {
    graphemeScriptEditor.ensureInitialTab();
    if (!graphemeScriptEditor.activeTab?.body.trim()) {
      graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
    } else {
      graphemeScriptEditor.openNewTab();
      graphemeScriptEditor.patchActiveTab(applyRecipeToEditor(recipe));
    }
    haptic("success");
    closeAll();
  }

  async function openScript(entry: GraphemeScriptEntry) {
    await graphemeScriptEditor.openScriptById(entry.id);
    haptic("light");
    closeAll();
  }

  function startNewScript() {
    graphemeScriptEditor.openNewTab();
    haptic("light");
    closeAll();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="mobile-sheet-backdrop scripts-workbench-tools-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeAll();
    }}
  >
    <div
      class="mobile-sheet mobile-sheet-tall scripts-workbench-tools-sheet flex flex-col"
      role="dialog"
      aria-label={sheetTitle}
    >
      <header class="mobile-sheet-header scripts-workbench-sheet-header shrink-0">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="flex items-center gap-2">
          {#if view !== "root"}
            <button
              type="button"
              class="mobile-turn-sheet-icon-btn"
              aria-label="Back"
              onclick={goBack}
            >
              <ChevronLeft size={18} strokeWidth={2} />
            </button>
          {/if}
          <h2 class="min-w-0 flex-1 truncate text-sm font-medium text-surface-100">
            {sheetTitle}
          </h2>
          <button type="button" class="workshop-text-action text-xs" onclick={closeAll}>
            Done
          </button>
        </div>
      </header>

      {#if view !== "root" && view !== "chat"}
        <div class="shrink-0 px-3 py-2">
          <input
            class="input w-full text-xs"
            type="search"
            placeholder={view === "templates"
              ? "Search templates…"
              : view === "library"
                ? "Search saved scripts…"
                : "Search modules or actions…"}
            bind:value={search}
          />
        </div>
      {/if}

      {#if view === "chat"}
        <div class="flex min-h-0 w-full min-w-0 flex-1 flex-col overflow-hidden">
          <ScriptWorkbenchChatPanel
            {visible}
            mobile={true}
            onOpenFullChat={() => {
              closeAll();
              layout.navigateDesktop("chat", { bump: true });
            }}
          />
        </div>
      {:else}
      <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto">
        {#if view === "root"}
          <div class="mobile-turn-sheet-group">
            <button
              type="button"
              class="mobile-turn-sheet-link-row"
              onclick={() => goTo("templates")}
            >
              <span class="flex items-center gap-2">
                <LayoutTemplate size={16} strokeWidth={1.75} class="text-primary-300" />
                <span class="mobile-turn-sheet-link-label">Templates</span>
              </span>
              <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
              onclick={() => goTo("library")}
            >
              <span class="flex items-center gap-2">
                <FileCode2 size={16} strokeWidth={1.75} class="text-primary-300" />
                <span class="mobile-turn-sheet-link-label">Library</span>
              </span>
              <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
              onclick={() => goTo("modules-list")}
            >
              <span class="flex items-center gap-2">
                <Blocks size={16} strokeWidth={1.75} class="text-primary-300" />
                <span class="mobile-turn-sheet-link-label">Modules</span>
              </span>
              <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
              onclick={() => goTo("chat")}
            >
              <span class="flex items-center gap-2">
                <MessageSquare size={16} strokeWidth={1.75} class="text-primary-300" />
                <span class="mobile-turn-sheet-link-label">Script chat</span>
              </span>
              <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
            </button>
          </div>
        {:else if view === "templates"}
          {#if filteredRecipes.length === 0}
            <p class="workshop-muted px-3 py-4 text-xs">No templates match.</p>
          {:else}
            <ul class="divide-y divide-surface-500/35">
              {#each filteredRecipes as recipe (recipe.id)}
                <li>
                  <button
                    type="button"
                    class="flex w-full flex-col px-4 py-3 text-left active:bg-surface-800/70"
                    onclick={() => applyTemplate(recipe)}
                  >
                    <span class="text-sm font-medium text-surface-100">{recipe.title}</span>
                    <span class="workshop-faint mt-0.5 text-[11px]">{recipe.subtitle}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {:else if view === "library"}
          <div class="px-4 pb-2 pt-1">
            <button type="button" class="workshop-text-action text-xs" onclick={startNewScript}>
              + New script
            </button>
          </div>
          {#if filteredScripts.length === 0}
            <p class="workshop-muted px-4 py-4 text-xs">No saved scripts yet.</p>
          {:else}
            <ul class="divide-y divide-surface-500/35">
              {#each filteredScripts as entry (entry.id)}
                <li>
                  <button
                    type="button"
                    class="flex w-full flex-col px-4 py-3 text-left active:bg-surface-800/70"
                    onclick={() => void openScript(entry)}
                  >
                    <span class="truncate text-sm font-medium text-surface-100">{entry.name}</span>
                    <span class="workshop-faint mt-0.5 truncate font-mono text-[10px]">
                      {entry.id}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {:else if view === "modules-list"}
          {#if filteredModules.length === 0}
            <p class="workshop-muted px-4 py-4 text-xs">No modules match.</p>
          {:else}
            <ul class="divide-y divide-surface-500/35">
              {#each filteredModules as entry (entry.module_id)}
                <li>
                  <button
                    type="button"
                    class="flex w-full items-center gap-3 px-4 py-3 text-left active:bg-surface-800/70"
                    onclick={() => selectModule(entry)}
                  >
                    <span class="min-w-0 flex-1 truncate font-mono text-sm text-surface-100">
                      {entry.module_id}
                    </span>
                    <span class="workshop-faint shrink-0 text-[10px] tabular-nums">
                      {entry.op_count} ops
                    </span>
                    <ChevronRight size={14} strokeWidth={2} class="shrink-0 text-surface-500" />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {:else if view === "modules-detail" && selectedModule}
          <div class="border-b border-surface-500/35 px-4 py-3">
            <p class="workshop-faint text-[11px] leading-snug">{moduleBlurb(selectedModule)}</p>
            <div class="mt-2 flex flex-wrap gap-1">
              <span class="scripts-workbench-meta-chip">v{selectedModule.version}</span>
              <span class="scripts-workbench-meta-chip">{selectedModule.op_count} ops</span>
              {#each selectedModule.effects as effect (effect)}
                <span class="scripts-workbench-effect-chip {effectBadgeClass(effect)}">
                  {effect}
                </span>
              {/each}
            </div>
          </div>
          {#if workshop.moduleDetailLoading && !moduleDetailForSelected}
            <p class="workshop-muted px-4 py-4 text-xs">Loading actions…</p>
          {:else if workshop.moduleDetailError}
            <p class="px-4 py-4 text-xs text-error-400">{workshop.moduleDetailError}</p>
          {:else if filteredModuleOps.length === 0}
            <p class="workshop-muted px-4 py-4 text-xs">No actions match.</p>
          {:else}
            <ul class="space-y-2 p-3">
              {#each filteredModuleOps as op (op.op)}
                <li>
                  <button
                    type="button"
                    class="scripts-workbench-op-card w-full rounded-md border border-surface-500/35 px-3 py-2.5 text-left active:border-primary-500/30 active:bg-surface-800/60"
                    onclick={() => insertOp(op.op)}
                  >
                    <p class="truncate font-mono text-[12px] text-surface-100">{op.op}()</p>
                    <p class="workshop-faint mt-1 truncate text-[10px]">→ {op.output_type}</p>
                    <div class="mt-1.5 flex flex-wrap gap-1">
                      <span class="scripts-workbench-effect-chip {effectBadgeClass(op.effect)}">
                        {op.effect}
                      </span>
                      <span class="scripts-workbench-meta-chip">{stabilityLabel(op)}</span>
                    </div>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </div>
      {/if}
    </div>
  </div>
{/if}

{#if !open}
  <button
    type="button"
    class="mobile-fab scripts-workbench-fab"
    aria-label="Script tools"
    onclick={() => {
      haptic("medium");
      onOpen();
    }}
  >
    <Plus size={24} strokeWidth={2} />
  </button>
{/if}
