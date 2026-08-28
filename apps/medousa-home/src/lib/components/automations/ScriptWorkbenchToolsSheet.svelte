<script lang="ts">
  import {
    ChevronLeft,
    ChevronRight,
    GitBranchPlus,
    Hammer,
    LayoutTemplate,
    MessageSquare,
    Pencil,
    Plus,
    Terminal,
    Zap,
  } from "@lucide/svelte";
  import ScriptWorkbenchChatPanel from "$lib/components/automations/ScriptWorkbenchChatPanel.svelte";
  import {
    GRAPHEME_STARTER_RECIPES,
    type GraphemeRecipe,
  } from "$lib/grapheme/graphemeRecipes";
  import { haptic } from "$lib/haptics";
  import { persistScriptName } from "$lib/grapheme/scriptWorkbenchActions";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import { layout } from "$lib/runtime/layout.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import type { GraphemeScriptEntry } from "$lib/types/grapheme";

  interface Props {
    open: boolean;
    visible: boolean;
    initialView?: ToolsView;
    hideFab?: boolean;
    onOpen: () => void;
    onClose: () => void;
    onApplyTemplate: (recipe: GraphemeRecipe) => void;
    onOpenScript: (entry: GraphemeScriptEntry) => void | Promise<void>;
    onNewScript: () => void;
    onInserted?: () => void;
  }

  type ToolsView = "root" | "templates" | "library" | "chat" | "rename";

  let {
    open,
    visible,
    initialView = "root",
    hideFab = false,
    onOpen,
    onClose,
    onApplyTemplate,
    onOpenScript,
    onNewScript,
  }: Props = $props();

  let view = $state<ToolsView>("root");
  let search = $state("");
  let renameDraft = $state("");
  let renameBusy = $state(false);
  let renameError = $state<string | null>(null);
  let renameInput = $state<HTMLInputElement | null>(null);
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  $effect(() => {
    if (!open) {
      view = "root";
      search = "";
      return;
    }
    view = initialView;
    search = "";
  });

  const sheetTitle = $derived(
    view === "root"
      ? "Script actions"
      : view === "templates"
        ? "Templates"
        : view === "library"
          ? "Library"
          : view === "chat"
            ? "Script chat"
            : view === "rename"
              ? "Rename script"
            : "Script tools",
  );

  const activeTab = $derived(graphemeScriptEditor.activeTab);
  const hasSource = $derived(Boolean(activeTab?.body.trim()));

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

  function closeAll() {
    haptic("light");
    onClose();
  }

  function goTo(next: ToolsView) {
    haptic("light");
    search = "";
    if (next === "rename") {
      renameDraft = activeTab?.name ?? "";
      renameError = null;
    }
    view = next;
    if (next === "rename") {
      requestAnimationFrame(() => {
        renameInput?.focus();
        renameInput?.select();
      });
    }
  }

  function goBack() {
    haptic("light");
    view = "root";
    search = "";
  }

  function applyTemplate(recipe: GraphemeRecipe) {
    onApplyTemplate(recipe);
    haptic("success");
    closeAll();
  }

  async function openScript(entry: GraphemeScriptEntry) {
    try {
      await onOpenScript(entry);
      haptic("light");
      closeAll();
    } catch (err) {
      console.error("Failed to open script", err);
      haptic("warning");
    }
  }

  function startNewScript() {
    onNewScript();
    haptic("light");
    closeAll();
  }

  function runAction(eventName: string) {
    haptic("light");
    window.dispatchEvent(new CustomEvent(eventName));
    onClose();
  }

  async function commitRename() {
    if (!activeTab || renameBusy) return;
    renameBusy = true;
    renameError = null;
    try {
      await persistScriptName(activeTab, renameDraft);
      haptic("success");
      onClose();
    } catch (err) {
      renameError = err instanceof Error ? err.message : String(err);
      haptic("warning");
    } finally {
      renameBusy = false;
    }
  }

  function handleSheetSwipeBack(): boolean {
    if (view === "root") return false;
    view = "root";
    search = "";
    return true;
  }

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      if (view === "root") {
        onClose();
        return true;
      }
      return handleSheetSwipeBack();
    });
  });

  $effect(() => {
    if (!open || !sheetEl) return;
    // No horizontal swipe-back: list taps often include sideways drift on iOS.
    return attachMobileSheetGestures(sheetEl, headerEl, {
      onDismiss: closeAll,
      swipeBack: false,
    });
  });
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
      bind:this={sheetEl}
      class="mobile-sheet {view === 'root'
        ? 'scripts-workbench-actions-sheet'
        : view === 'rename'
          ? 'mobile-sheet-medium'
          : 'mobile-sheet-tall'} automations-sheet scripts-workbench-tools-sheet flex flex-col"
      role="dialog"
      aria-label={sheetTitle}
    >
      <header bind:this={headerEl} class="mobile-sheet-stack-header">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="mobile-sheet-header-row">
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

      {#if view === "templates" || view === "library"}
        <div class="automation-sheet-search shrink-0 py-2">
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
      <div class="mobile-sheet-scroll">
        {#if view === "root"}
          <div class="mobile-turn-sheet-group">
            <button
              type="button"
              class="mobile-turn-sheet-link-row"
              disabled={!activeTab}
              onclick={() => goTo("rename")}
            >
              <span class="flex items-center gap-2">
                <Pencil size={16} strokeWidth={1.75} class="text-content-link" />
                <span class="mobile-turn-sheet-link-label">Rename</span>
              </span>
              <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
              disabled={!hasSource || graphemeScriptEditor.compileBusy}
              onclick={() => runAction("medousa-mobile-script-compile")}
            >
              <span class="flex items-center gap-2">
                <Hammer size={16} strokeWidth={1.75} class="text-content-link" />
                <span class="mobile-turn-sheet-link-label">Compile check</span>
              </span>
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
              disabled={!hasSource || graphemeScriptEditor.compileBusy}
              onclick={() => runAction("medousa-mobile-script-optimize")}
            >
              <span class="flex items-center gap-2">
                <Zap size={16} strokeWidth={1.75} class="text-content-link" />
                <span class="mobile-turn-sheet-link-label">Optimize AOT</span>
              </span>
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
              disabled={!hasSource}
              onclick={() => runAction("medousa-mobile-script-add-flow")}
            >
              <span class="flex items-center gap-2">
                <GitBranchPlus size={16} strokeWidth={1.75} class="text-content-link" />
                <span class="mobile-turn-sheet-link-label">Add to flow</span>
              </span>
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
              onclick={() => runAction("medousa-mobile-script-output")}
            >
              <span class="flex items-center gap-2">
                <Terminal size={16} strokeWidth={1.75} class="text-content-link" />
                <span class="mobile-turn-sheet-link-label">View output</span>
              </span>
            </button>
          </div>
          <div class="mobile-turn-sheet-group mobile-turn-sheet-group-secondary">
            <button
              type="button"
              class="mobile-turn-sheet-link-row"
              onclick={() => goTo("templates")}
            >
              <span class="flex items-center gap-2">
                <LayoutTemplate size={16} strokeWidth={1.75} class="text-content-link" />
                <span class="mobile-turn-sheet-link-label">Templates</span>
              </span>
              <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
              onclick={() => goTo("chat")}
            >
              <span class="flex items-center gap-2">
                <MessageSquare size={16} strokeWidth={1.75} class="text-content-link" />
                <span class="mobile-turn-sheet-link-label">Script chat</span>
              </span>
              <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
            </button>
          </div>
        {:else if view === "rename"}
          <form
            class="space-y-3 px-1 py-2"
            onsubmit={(event) => {
              event.preventDefault();
              void commitRename();
            }}
          >
            <label class="block">
              <span class="workshop-label">Name</span>
              <input
                bind:this={renameInput}
                class="input mt-1 w-full text-sm"
                type="text"
                bind:value={renameDraft}
                autocomplete="off"
                spellcheck="false"
              />
            </label>
            {#if renameError}
              <p class="text-xs text-content-error">{renameError}</p>
            {/if}
            <button
              type="submit"
              class="btn variant-filled-primary w-full justify-center"
              disabled={renameBusy || !activeTab}
            >
              {renameBusy ? "Renaming…" : "Rename"}
            </button>
          </form>
        {:else if view === "templates"}
          {#if filteredRecipes.length === 0}
            <p class="workshop-muted px-3 py-4 text-xs">No templates match.</p>
          {:else}
            <ul class="mobile-turn-sheet-group">
              {#each filteredRecipes as recipe, index (recipe.id)}
                <li>
                  <button
                    type="button"
                    class="mobile-turn-sheet-row {index > 0
                      ? 'mobile-turn-sheet-row-divider'
                      : ''}"
                    style="touch-action: manipulation"
                    onclick={() => applyTemplate(recipe)}
                  >
                    <span class="mobile-turn-sheet-row-copy">
                      <span class="mobile-turn-sheet-row-title">{recipe.title}</span>
                      <span class="mobile-turn-sheet-row-subtitle">{recipe.subtitle}</span>
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {:else if view === "library"}
          {#if filteredScripts.length === 0}
            <div class="mobile-turn-sheet-group">
              <button type="button" class="mobile-turn-sheet-row" onclick={startNewScript}>
                <Plus size={18} strokeWidth={1.8} class="shrink-0 text-content-link" />
                <span class="mobile-turn-sheet-row-copy">
                  <span class="mobile-turn-sheet-row-title">New script</span>
                </span>
              </button>
            </div>
            <p class="workshop-muted px-1 py-4 text-xs">No saved scripts yet.</p>
          {:else}
            <ul class="mobile-turn-sheet-group">
              <li>
                <button type="button" class="mobile-turn-sheet-row" onclick={startNewScript}>
                  <Plus size={18} strokeWidth={1.8} class="shrink-0 text-content-link" />
                  <span class="mobile-turn-sheet-row-copy">
                    <span class="mobile-turn-sheet-row-title">New script</span>
                  </span>
                </button>
              </li>
              {#each filteredScripts as entry (entry.id)}
                <li>
                  <button
                    type="button"
                    class="mobile-turn-sheet-row mobile-turn-sheet-row-divider"
                    style="touch-action: manipulation"
                    onclick={() => void openScript(entry)}
                  >
                    <span class="mobile-turn-sheet-row-copy">
                      <span class="mobile-turn-sheet-row-title truncate">{entry.name}</span>
                      <span class="mobile-turn-sheet-row-subtitle truncate font-mono">
                        {entry.id}
                      </span>
                    </span>
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

{#if !open && !hideFab}
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
