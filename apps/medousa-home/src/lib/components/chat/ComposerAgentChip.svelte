<script lang="ts">
  import { Bot, ChevronDown } from "@lucide/svelte";
  import { activeAgent } from "$lib/stores/activeAgent.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

  let open = $state(false);

  const label = $derived.by(() => {
    const id = activeAgent.selectedManuscriptId;
    if (!id) return "Medousa";
    return catalog.manuscripts.find((entry) => entry.id === id)?.name ?? id;
  });

  $effect(() => {
    if (open && catalog.manuscripts.length === 0 && !catalog.loading) {
      void catalog.refresh();
    }
  });

  function pickDefault() {
    activeAgent.clear();
    open = false;
  }

  function pickAgent(id: string) {
    activeAgent.setActive(id);
    open = false;
  }

  function openAgentsInWorkspace() {
    open = false;
    lmeWorkspace.setExplorerMode("agents");
    layout.navigateDesktop("library");
  }
</script>

<button
  type="button"
  class="composer-agent-chip inline-flex max-w-[10rem] shrink-0 items-center gap-1 rounded-md border border-surface-500/40 bg-surface-900/70 px-2 py-1 text-xs text-surface-200 transition hover:border-primary-500/40 hover:text-surface-50"
  aria-label="Active agent — {label}"
  aria-haspopup="dialog"
  aria-expanded={open}
  onclick={() => (open = true)}
>
  <Bot size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
  <span class="truncate font-medium">{label}</span>
  <ChevronDown size={12} class="shrink-0 text-surface-500" strokeWidth={2} />
</button>

{#if open}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) open = false;
    }}
  >
    <div class="mobile-sheet" role="dialog" aria-label="Choose agent">
      <header class="mobile-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">Who runs this</h2>
          <p class="workshop-faint mt-0.5 text-xs">Default Medousa or a specialist agent</p>
        </div>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface shrink-0"
          onclick={() => (open = false)}
        >
          Done
        </button>
      </header>
      <div class="mobile-you-scroll space-y-1 px-3 pb-4">
        <button
          type="button"
          class="settings-toggle-row w-full text-left {activeAgent.selectedManuscriptId === null
            ? 'workshop-list-row-active'
            : ''}"
          onclick={pickDefault}
        >
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">Medousa</span>
            <span class="workshop-faint mt-0.5 block text-xs">Default — no specialist</span>
          </span>
        </button>
        {#each catalog.manuscripts as entry (entry.id)}
          <button
            type="button"
            class="settings-toggle-row w-full text-left {activeAgent.selectedManuscriptId ===
            entry.id
              ? 'workshop-list-row-active'
              : ''}"
            onclick={() => pickAgent(entry.id)}
          >
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm font-medium text-surface-100">{entry.name}</span>
              {#if entry.description}
                <span class="workshop-faint mt-0.5 block truncate text-xs">{entry.description}</span>
              {/if}
            </span>
          </button>
        {/each}
        <button
          type="button"
          class="workshop-text-action mt-3 text-xs"
          onclick={openAgentsInWorkspace}
        >
          Manage agents in Workspace…
        </button>
      </div>
    </div>
  </div>
{/if}
