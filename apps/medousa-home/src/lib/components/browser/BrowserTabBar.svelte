<script lang="ts">
  import { Plus, X } from "@lucide/svelte";
  import { browser } from "$lib/stores/browser.svelte";

  function handleNewTab() {
    void browser.openTab("about:blank", "user");
  }
</script>

<div class="flex min-w-0 items-end gap-1 overflow-x-auto border-b border-surface-800 bg-surface-950/90 px-2 pt-2">
  {#each browser.tabs as tab (tab.id)}
    <button
      type="button"
      class="group flex max-w-[220px] min-w-[120px] items-center gap-1 rounded-t-md border px-2 py-1.5 text-left text-xs {tab.active
        ? 'border-surface-600 border-b-transparent bg-surface-900 text-surface-50'
        : 'border-transparent bg-surface-900/40 text-surface-300 hover:bg-surface-900/70'}"
      onclick={() => void browser.activateTab(tab.id)}
      title={tab.url}
    >
      <span
        class="inline-block h-1.5 w-1.5 shrink-0 rounded-full {tab.opened_by === 'agent'
          ? 'bg-primary-400'
          : 'bg-surface-500'}"
        aria-hidden="true"
      ></span>
      <span class="truncate">{tab.title || tab.url || "New tab"}</span>
      {#if browser.tabs.length > 1}
        <span
          role="button"
          tabindex="0"
          class="ml-auto shrink-0 rounded p-0.5 text-surface-500 opacity-0 group-hover:opacity-100 hover:bg-surface-800 hover:text-surface-100"
          aria-label="Close tab"
          onclick={(event) => {
            event.stopPropagation();
            void browser.closeTab(tab.id);
          }}
          onkeydown={(event) => {
            if (event.key !== "Enter" && event.key !== " ") return;
            event.preventDefault();
            event.stopPropagation();
            void browser.closeTab(tab.id);
          }}
        >
          <X size={12} />
        </span>
      {/if}
    </button>
  {/each}
  <button
    type="button"
    class="mb-1 rounded-md p-1.5 text-surface-400 hover:bg-surface-800 hover:text-surface-100"
    aria-label="New tab"
    title="New tab"
    onclick={handleNewTab}
  >
    <Plus size={14} />
  </button>
</div>
