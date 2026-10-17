<script lang="ts">
  import { House, Plus } from "@lucide/svelte";
  import type { SpotlightLens } from "$lib/commands/collectCommands";
  let { scopeTabs, selectedScopeId, selectedLens, lensTabs, query, notesMode, activityMode, selectScope, selectLens, createWorkspace }: {
    scopeTabs: Array<{ id: string; label: string }>;
    selectedScopeId: string;
    selectedLens: SpotlightLens;
    lensTabs: Array<{ id: SpotlightLens; label: string }>;
    query: string;
    notesMode: boolean;
    activityMode: boolean;
    selectScope: (id: string) => void;
    selectLens: (lens: SpotlightLens) => void;
    createWorkspace: () => void;
  } = $props();
</script>

<div class="command-spotlight-navigation">
  <nav class="command-spotlight-scopes" aria-label="Workshop scopes">
    <span class="command-spotlight-scopes-label">Search in</span>
    <div class="command-spotlight-scope-list" role="tablist" aria-label="Workspaces">
      {#each scopeTabs as scope (scope.id)}
        <button
          type="button"
          role="tab"
          class="command-spotlight-scope"
          class:command-spotlight-scope-active={scope.id === selectedScopeId}
          aria-selected={scope.id === selectedScopeId}
          onclick={() => selectScope(scope.id)}
        >
          <span class="command-spotlight-scope-content">
            {#if scope.id === "home"}
              <House size={12} strokeWidth={1.7} />
            {/if}
            <span>{scope.label}</span>
          </span>
        </button>
      {/each}
      <button
        type="button"
        class="command-spotlight-workspace-add"
        aria-label="Create workspace"
        title="Create workspace"
        onclick={createWorkspace}
      >
        <Plus size={13} strokeWidth={1.7} />
      </button>
    </div>
  </nav>

  {#if selectedScopeId === "home" && !notesMode}
    {#if query.trim() && !activityMode}
      <span class="command-spotlight-searching-all">All results</span>
    {:else}
      <nav class="command-spotlight-lenses" aria-label="Spotlight categories">
        {#each lensTabs as lens (lens.id)}
          <button
            type="button"
            class="command-spotlight-lens"
            class:command-spotlight-lens-active={lens.id === selectedLens}
            aria-current={lens.id === selectedLens ? "page" : undefined}
            onclick={() => selectLens(lens.id)}
          >
            <span class="command-spotlight-lens-content">{lens.label}</span>
          </button>
        {/each}
      </nav>
    {/if}
  {/if}
</div>
